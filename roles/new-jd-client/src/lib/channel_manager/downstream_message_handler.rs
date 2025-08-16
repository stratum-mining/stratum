use stratum_common::roles_logic_sv2::{
    bitcoin::{Amount, TxOut},
    channels_sv2::server::{extended::ExtendedChannel, jobs::job_store::DefaultJobStore},
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{
        HandleMiningMessagesFromClientAsync, HandlerError as Error, SupportedChannelTypes,
    },
    mining_sv2::*,
    parsers_sv2::{AnyMessage, Mining},
    VardiffState,
};
use tracing::{info, warn};

use crate::{channel_manager::ChannelManager, error::JDCError, utils::StdFrame};

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled_for_client(&self) -> bool {
        false
    }
    fn is_client_authorized(&self, user_identity: &Str0255) -> Result<bool, Error> {
        Ok(true)
    }
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Error> {
        info!("Received handle_close_channel from Downstream");
        Ok(())
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_open_standard_mining_channel from Downstream");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .map_err(|e| Error::External(JDCError::InvalidUserIdentity(e.to_string()).into()))?;
        info!("Received OpenStandardMiningChannel, {msg}");
        let (last_future_template, last_new_prev_hash) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.last_future_template.clone(),
                    data.last_new_prev_hash.clone(),
                )
            });

        let last_future_template = last_future_template.unwrap();
        let last_new_prev_hash = last_new_prev_hash.unwrap();

        let pool_coinbase_output = TxOut {
            value: Amount::from_sat(last_future_template.coinbase_tx_value_remaining),
            script_pubkey: self.coinbase_reward_script.script_pubkey(),
        };

        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_open_extended_mining_channel from Downstream");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .unwrap();

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();
        let requested_min_rollable_extranonce_size = msg.min_extranonce_size;

        let (extranonce_prefix, channel_id, last_future_template, last_new_prev_hash) =
            self.channel_manager_data.super_safe_lock(|data| {
                let channel_id = data.channel_id_factory.next();
                let extranonce_prefix = data
                    .extranonce_prefix_factory_extended
                    .next_prefix_extended(requested_min_rollable_extranonce_size.into());
                let future_job = data.last_future_template.clone();
                let prevhash = data.last_new_prev_hash.clone();
                (extranonce_prefix, channel_id, future_job, prevhash)
            });

        if last_future_template.is_none() || last_new_prev_hash.is_none() {
            warn!("We are still initializing kindly wait");
            return Ok(());
        }

        let extranonce_prefix = extranonce_prefix.unwrap();
        let job_store = Box::new(DefaultJobStore::new());

        let mut extended_channel = ExtendedChannel::new_for_job_declaration_client(
            channel_id,
            user_identity,
            extranonce_prefix.into(),
            requested_max_target.into(),
            nominal_hash_rate,
            true,
            requested_min_rollable_extranonce_size,
            self.share_batch_size,
            self.shares_per_minute,
            job_store,
            self.pool_tag_string.clone(),
            self.miner_tag_string.clone(),
        )
        .unwrap();

        let open_extended_mining_channel_success = OpenExtendedMiningChannelSuccess {
            request_id,
            channel_id,
            target: extended_channel.get_target().clone().into(),
            extranonce_prefix: extended_channel
                .get_extranonce_prefix()
                .clone()
                .try_into()
                .unwrap(),
            extranonce_size: extended_channel.get_rollable_extranonce_size(),
        }
        .into_static();

        let last_future_template = last_future_template.unwrap();
        let last_set_new_prev_hash_tdp = last_new_prev_hash.unwrap();

        // get the script pubkey from pool, otherwise in case of solo mining use the config one
        let pool_coinbase_output = TxOut {
            value: Amount::from_sat(last_future_template.coinbase_tx_value_remaining),
            script_pubkey: self.coinbase_reward_script.script_pubkey(),
        };

        // create a future extended job based on the last future template
        extended_channel
            .on_new_template(last_future_template.clone(), vec![pool_coinbase_output])
            .unwrap();

        let future_extended_job_id = extended_channel
            .get_future_template_to_job_id()
            .get(&last_future_template.template_id)
            .expect("future job id must exist");
        let future_extended_job = extended_channel
            .get_future_jobs()
            .get(future_extended_job_id)
            .expect("future job must exist");

        let future_extended_job_message = Mining::NewExtendedMiningJob(
            future_extended_job.get_job_message().clone().into_static(),
        );
        self.channel_manager_channel
            .downstream_sender
            .send(AnyMessage::Mining(future_extended_job_message));

        let prev_hash = last_set_new_prev_hash_tdp.prev_hash.clone();
        let header_timestamp = last_set_new_prev_hash_tdp.header_timestamp;
        let n_bits = last_set_new_prev_hash_tdp.n_bits;
        let set_new_prev_hash_mining = Mining::SetNewPrevHash(SetNewPrevHash {
            channel_id,
            job_id: *future_extended_job_id,
            prev_hash,
            min_ntime: header_timestamp,
            nbits: n_bits,
        });
        extended_channel
            .on_set_new_prev_hash(last_set_new_prev_hash_tdp)
            .unwrap();
        self.channel_manager_channel
            .downstream_sender
            .send(AnyMessage::Mining(set_new_prev_hash_mining));

        let vardiff = Box::new(VardiffState::new().unwrap());

        self.channel_manager_data.super_safe_lock(|data| {
            data.extended_channels.insert(channel_id, extended_channel);
            data.vardiff.insert(channel_id, vardiff);
        });

        Ok(())
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Error> {
        info!("Received handle_update_channel from Downstream");
        Ok(())
    }

    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_standard from Downstream");
        Ok(())
    }

    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_submit_shares_extended from Downstream");
        Ok(())
    }

    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_set_custom_mining_job from Downstream");
        Ok(())
    }
}
