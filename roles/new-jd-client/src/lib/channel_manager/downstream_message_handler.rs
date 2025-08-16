use std::sync::{Arc, RwLock};

use stratum_common::roles_logic_sv2::{
    bitcoin::{Amount, TxOut},
    channels_sv2::server::{
        extended::ExtendedChannel,
        group::GroupChannel,
        jobs::job_store::DefaultJobStore,
        share_accounting::{ShareValidationError, ShareValidationResult},
        standard::StandardChannel,
    },
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{
        HandleMiningMessagesFromClientAsync, HandlerError as Error, SupportedChannelTypes,
    },
    job_declaration_sv2::PushSolution,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
    VardiffState,
};
use tracing::{error, info, warn};

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
        let user_string = msg.user_identity.as_utf8_or_hex();
        let mut split = user_string.split("#").collect::<Vec<&str>>();
        let downstream_id = split.pop().unwrap().parse::<u32>().unwrap();
        info!("Received handle_open_standard_mining_channel from Downstream");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .map_err(|e| Error::External(JDCError::InvalidUserIdentity(e.to_string()).into()))?;
        info!("Received OpenStandardMiningChannel, {msg}");
        let (last_future_template, last_new_prev_hash, is_group_channel, downstream) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.last_future_template.clone(),
                    data.last_new_prev_hash.clone(),
                    data.group_channel.is_none(),
                    data.downstream.get(&downstream_id).cloned(),
                )
            });
        if downstream.is_none() {
            warn!("Downstream should already be connected");
            return Ok(());
        }

        let downstream = downstream.unwrap();
        let last_future_template = last_future_template.unwrap();
        let last_new_prev_hash = last_new_prev_hash.unwrap();

        let pool_coinbase_output = TxOut {
            value: Amount::from_sat(last_future_template.coinbase_tx_value_remaining),
            script_pubkey: self.coinbase_reward_script.script_pubkey(),
        };

        if downstream
            .downstream_data
            .super_safe_lock(|data| data.require_std_job)
            && is_group_channel
        {
            let group_channel_id = self
                .channel_manager_data
                .super_safe_lock(|data| data.channel_id_factory.next());
            let job_store = Box::new(DefaultJobStore::new());
            let mut group_channel_ = GroupChannel::new_for_job_declaration_client(
                group_channel_id,
                job_store,
                self.pool_tag_string.clone(),
                self.miner_tag_string.clone(),
            );

            group_channel_
                .on_new_template(
                    last_future_template.clone(),
                    vec![pool_coinbase_output.clone()],
                )
                .unwrap();

            group_channel_
                .on_set_new_prev_hash(last_new_prev_hash.clone())
                .unwrap();

            let group_channel_ = Some(group_channel_);

            self.channel_manager_data.super_safe_lock(|data| {
                data.group_channel = group_channel_;
            });
        }

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();

        let (extranonce_prefix, channel_id, group_channel_id) =
            self.channel_manager_data.super_safe_lock(|data| {
                let mut group_id = 0;
                if let Some(ref group_channel) = data.group_channel {
                    group_id = group_channel.get_group_channel_id();
                }
                (
                    data.extranonce_prefix_factory_standard
                        .next_prefix_standard(),
                    data.channel_id_factory.next(),
                    group_id,
                )
            });
        let extranonce_prefix = extranonce_prefix.unwrap();
        let job_store = Box::new(DefaultJobStore::new());

        let mut standard_channel = StandardChannel::new_for_job_declaration_client(
            channel_id,
            user_identity,
            extranonce_prefix.to_vec(),
            requested_max_target.into(),
            nominal_hash_rate,
            self.share_batch_size,
            self.shares_per_minute,
            job_store,
            self.pool_tag_string.clone(),
            self.miner_tag_string.clone(),
        )
        .unwrap();

        let open_standard_mining_channel_success = OpenStandardMiningChannelSuccess {
            request_id: msg.request_id,
            channel_id,
            target: standard_channel.get_target().clone().into(),
            extranonce_prefix: standard_channel
                .get_extranonce_prefix()
                .clone()
                .try_into()
                .expect("extranonce_prefix must be valid"),
            group_channel_id,
        }
        .into_static();

        self.channel_manager_channel.downstream_sender.send((
            downstream_id,
            AnyMessage::Mining(Mining::OpenStandardMiningChannelSuccess(
                open_standard_mining_channel_success,
            )),
        ));

        standard_channel.on_new_template(
            last_future_template.clone(),
            vec![pool_coinbase_output.clone()],
        );

        let future_standard_job_id = standard_channel
            .get_future_template_to_job_id()
            .get(&last_future_template.template_id)
            .expect("future job id must exist");
        let future_standard_job = standard_channel
            .get_future_jobs()
            .get(future_standard_job_id)
            .expect("future job must exist");
        let future_standard_job_message =
            future_standard_job.get_job_message().clone().into_static();

        self.channel_manager_channel.downstream_sender.send((
            downstream_id,
            AnyMessage::Mining(Mining::NewMiningJob(future_standard_job_message)),
        ));

        let prev_hash = last_new_prev_hash.prev_hash.clone();
        let header_timestamp = last_new_prev_hash.header_timestamp;
        let n_bits = last_new_prev_hash.n_bits;
        let set_new_prev_hash_mining = SetNewPrevHash {
            channel_id,
            job_id: *future_standard_job_id,
            prev_hash,
            min_ntime: header_timestamp,
            nbits: n_bits,
        };

        standard_channel
            .on_set_new_prev_hash(last_new_prev_hash.clone())
            .unwrap();

        self.channel_manager_channel.downstream_sender.send((
            downstream_id,
            AnyMessage::Mining(Mining::SetNewPrevHash(set_new_prev_hash_mining)),
        ));

        let vardiff = VardiffState::new().unwrap();

        self.channel_manager_data.super_safe_lock(|data| {
            data.vardiff.insert(channel_id, Box::new(vardiff));
            data.standard_channels.insert(channel_id, standard_channel);
            data.channel_id_to_downstream_id
                .insert(channel_id, downstream_id);
            if let Some(ref mut group_id) = data.group_channel {
                group_id.add_standard_channel_id(channel_id);
            }
        });

        Ok(())
    }

    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Error> {
        let user_string = msg.user_identity.as_utf8_or_hex();
        let mut split = user_string.split("#").collect::<Vec<&str>>();
        let downstream_id = split.pop().unwrap().parse::<u32>().unwrap();
        info!("Received handle_open_extended_mining_channel from Downstream {downstream_id}");
        let request_id = msg.get_request_id_as_u32();
        let user_identity = std::str::from_utf8(msg.user_identity.as_ref())
            .map(|s| s.to_string())
            .unwrap();

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();
        let requested_min_rollable_extranonce_size = msg.min_extranonce_size;

        let (extranonce_prefix, channel_id, last_future_template, last_new_prev_hash, downstream) =
            self.channel_manager_data.super_safe_lock(|data| {
                let channel_id = data.channel_id_factory.next();
                let extranonce_prefix = data
                    .extranonce_prefix_factory_extended
                    .next_prefix_extended(requested_min_rollable_extranonce_size.into());
                let future_job = data.last_future_template.clone();
                let prevhash = data.last_new_prev_hash.clone();
                let downstream = data.downstream.get(&downstream_id).cloned();
                (
                    extranonce_prefix,
                    channel_id,
                    future_job,
                    prevhash,
                    downstream,
                )
            });

        if downstream.is_none() {
            warn!("Downstream should already be connected");
            return Ok(());
        }

        if last_future_template.is_none() || last_new_prev_hash.is_none() {
            warn!("We are still initializing kindly wait");
            return Ok(());
        }

        let downstream = downstream.unwrap();
        let extranonce_prefix = extranonce_prefix.unwrap();
        let job_store = Box::new(DefaultJobStore::new());

        if downstream
            .downstream_data
            .super_safe_lock(|data| data.require_std_job)
        {
            warn!(
                "You asked for standard jobs, and not for extended, kindly open standard channel"
            );
            return Ok(());
        }

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
        self.channel_manager_channel.downstream_sender.send((
            downstream_id,
            AnyMessage::Mining(future_extended_job_message),
        ));

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
            .send((downstream_id, AnyMessage::Mining(set_new_prev_hash_mining)));

        let vardiff = Box::new(VardiffState::new().unwrap());

        self.channel_manager_data.super_safe_lock(|data| {
            data.extended_channels.insert(channel_id, extended_channel);
            data.channel_id_to_downstream_id
                .insert(channel_id, downstream_id);
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
        let channel_id = msg.channel_id;
        let (downstream, is_extended_channel, prev_hash) =
            self.channel_manager_data.super_safe_lock(|data| {
                (
                    data.downstream.get(&msg.channel_id).cloned(),
                    data.extended_channels.get(&msg.channel_id).is_none(),
                    data.last_new_prev_hash.clone(),
                )
            });
        if downstream.is_none() {
            return Ok(());
        }
        let prev_hash = prev_hash.unwrap();
        let downstream = downstream.unwrap();
        if self
            .channel_manager_data
            .super_safe_lock(|data| data.extended_channels.get(&msg.channel_id).is_none())
        {
            let error = SubmitSharesError {
                channel_id,
                sequence_number: msg.sequence_number,
                error_code: "invalid-channel-id"
                    .to_string()
                    .try_into()
                    .expect("Error code must be a valid string"),
            };
            error!("SubmitSharesError: channel_id: {channel_id}, sequence_number: {}, error_code: invalid-channel-id", msg.sequence_number);
            self.channel_manager_channel.downstream_sender.send((
                downstream.downstream_id,
                AnyMessage::Mining(Mining::SubmitSharesError(error)),
            ));
        }

        let res = self.channel_manager_data.super_safe_lock(|data| {
            let mut extended_channel = data.extended_channels.get_mut(&msg.channel_id).unwrap();
            let mut vardiff = data.vardiff.get_mut(&msg.channel_id).unwrap();
            vardiff.increment_shares_since_last_update();
            extended_channel.validate_share(msg.clone())
        });

        match res {
            Ok(ShareValidationResult::Valid) => {
                info!(
                    "SubmitSharesExtended: valid share | channel_id: {}, sequence_number: {} ☑️",
                    channel_id, msg.sequence_number
                );
            }
            Ok(ShareValidationResult::ValidWithAcknowledgement(
                last_sequence_number,
                new_submits_accepted_count,
                new_shares_sum,
            )) => {
                let success = SubmitSharesSuccess {
                    channel_id,
                    last_sequence_number,
                    new_submits_accepted_count,
                    new_shares_sum,
                };
                info!("SubmitSharesExtended: {} ✅", success);
                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                ));
            }
            Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                info!("SubmitSharesExtended: 💰 Block Found!!! 💰");
                // if we have a template id (i.e.: this was not a custom job)
                // we can propagate the solution to the TP
                if let Some(template_id) = template_id {
                    info!("SubmitSharesExtended: Propagating solution to the Template Provider.");
                    let solution = SubmitSolution {
                        template_id,
                        version: msg.version,
                        header_timestamp: msg.ntime,
                        header_nonce: msg.nonce,
                        coinbase_tx: coinbase.try_into().unwrap(),
                    };

                    let push_solution = PushSolution {
                        extranonce: msg.extranonce,
                        ntime: msg.ntime,
                        nonce: msg.nonce,
                        version: msg.version,
                        nbits: prev_hash.n_bits,
                        prev_hash: prev_hash.prev_hash,
                    };

                    let any_message = AnyMessage::TemplateDistribution(
                        TemplateDistribution::SubmitSolution(solution.clone()),
                    );
                    let frame: StdFrame = any_message.try_into().unwrap();
                    self.channel_manager_channel
                        .tp_sender
                        .send(frame.into())
                        .await;
                    let any_message =
                        AnyMessage::JobDeclaration(JobDeclaration::PushSolution(push_solution))
                            .into_static();
                    let frame: StdFrame = any_message.try_into().unwrap();
                    self.channel_manager_channel
                        .jd_sender
                        .send(frame.into())
                        .await;
                }
                let share_accounting = self.channel_manager_data.super_safe_lock(|data| {
                    let extended_channel = data.extended_channels.get_mut(&msg.channel_id).unwrap();
                    extended_channel.get_share_accounting().clone()
                });
                let success = SubmitSharesSuccess {
                    channel_id,
                    last_sequence_number: share_accounting.get_last_share_sequence_number(),
                    new_submits_accepted_count: share_accounting.get_shares_accepted(),
                    new_shares_sum: share_accounting.get_share_work_sum(),
                };

                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesSuccess(success)),
                ));
            }
            Err(ShareValidationError::Invalid) => {
                error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-share ❌", channel_id, msg.sequence_number);
                let error = SubmitSharesError {
                    channel_id: msg.channel_id,
                    sequence_number: msg.sequence_number,
                    error_code: "invalid-share"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesError(error)),
                ));
            }
            Err(ShareValidationError::Stale) => {
                error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: stale-share ❌", channel_id, msg.sequence_number);
                let error = SubmitSharesError {
                    channel_id: msg.channel_id,
                    sequence_number: msg.sequence_number,
                    error_code: "stale-share"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesError(error)),
                ));
            }
            Err(ShareValidationError::InvalidJobId) => {
                error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: invalid-job-id ❌", channel_id, msg.sequence_number);
                let error = SubmitSharesError {
                    channel_id: msg.channel_id,
                    sequence_number: msg.sequence_number,
                    error_code: "invalid-job-id"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesError(error)),
                ));
            }
            Err(ShareValidationError::DoesNotMeetTarget) => {
                error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: difficulty-too-low ❌", channel_id, msg.sequence_number);
                let error = SubmitSharesError {
                    channel_id: msg.channel_id,
                    sequence_number: msg.sequence_number,
                    error_code: "difficulty-too-low"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
            }
            Err(ShareValidationError::DuplicateShare) => {
                error!("SubmitSharesError: channel_id: {}, sequence_number: {}, error_code: duplicate-share ❌", channel_id, msg.sequence_number);
                let error = SubmitSharesError {
                    channel_id: msg.channel_id,
                    sequence_number: msg.sequence_number,
                    error_code: "duplicate-share"
                        .to_string()
                        .try_into()
                        .expect("error code must be valid string"),
                };
                self.channel_manager_channel.downstream_sender.send((
                    downstream.downstream_id,
                    AnyMessage::Mining(Mining::SubmitSharesError(error)),
                ));
            }
            _ => {
                // any other error variations should never happen
                unreachable!()
            }
        }

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
