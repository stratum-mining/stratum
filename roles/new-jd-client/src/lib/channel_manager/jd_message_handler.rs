use std::time::Duration;

use stratum_common::roles_logic_sv2::{
    bitcoin::{
        self, absolute::LockTime, transaction::Version, OutPoint, ScriptBuf, Sequence, Transaction,
        TxIn, TxOut, Witness,
    },
    channels_sv2::chain_tip::ChainTip,
    codec_sv2::binary_sv2::{self, Sv2DataType, B016M},
    handlers_sv2::{HandleJobDeclarationMessagesFromServerAsync, HandlerError as Error},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
    },
    mining_sv2::SetCustomMiningJob,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::CoinbaseOutputConstraints,
};
use tracing::{debug, info, instrument, warn};

use crate::{channel_manager::ChannelManager, error::JDCError, utils::StdFrame};

impl HandleJobDeclarationMessagesFromServerAsync for ChannelManager {
    #[instrument(skip_all, fields(request_id = msg.request_id))]
    async fn handle_allocate_mining_job_token_success(
        &mut self,
        msg: AllocateMiningJobTokenSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Handling AllocateMiningJobTokenSuccess from JDS");

        let coinbase_changed = self.channel_manager_data.super_safe_lock(|data| {
            let changed = data.coinbase_outputs != msg.coinbase_outputs.to_vec();
            data.coinbase_outputs = msg.coinbase_outputs.to_vec();
            data.allocate_tokens = Some(msg.clone().into_static());
            changed
        });

        if coinbase_changed {
            debug!("Coinbase outputs changed, recalculating constraints");

            let deserialized: Vec<TxOut> =
                bitcoin::consensus::deserialize(&msg.coinbase_outputs.to_vec())
                    .map_err(|e| Error::External(Box::new(JDCError::BitcoinEncodeError(e))))?;

            let max_additional_size: usize = deserialized.iter().map(|o| o.size()).sum();

            let dummy_coinbase = Transaction {
                version: Version::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::from(vec![vec![0; 32]]),
                }],
                output: deserialized,
            };

            let max_additional_sigops = dummy_coinbase.total_sigop_cost(|_| None) as u16;

            debug!(
                max_additional_size,
                max_additional_sigops, "Computed coinbase output constraints"
            );

            let response = AnyMessage::TemplateDistribution(
                TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
                    coinbase_output_max_additional_size: max_additional_size as u32,
                    coinbase_output_max_additional_sigops: max_additional_sigops,
                }),
            );

            let frame: StdFrame = response
                .try_into()
                .map_err(|e| Error::External(Box::new(JDCError::Parser(e))))?;

            self.channel_manager_channel
                .tp_sender
                .send(frame.into())
                .await
                .map_err(|e| Error::External(Box::new(JDCError::ChannelErrorSender)))?;

            info!("Sent updated CoinbaseOutputConstraints to TP channel");
        } else {
            debug!("Coinbase outputs unchanged, skipping constraints update");
        }

        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id))]
    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError<'_>,
    ) -> Result<(), Error> {
        warn!(
            error_code = ?msg.error_code,
            "Received DeclareMiningJobError from JDS"
        );

        debug!("Fallback path triggered for request_id={}", msg.request_id);
        Error::External(JDCError::Shutdown.into());

        Ok(())
    }

    #[instrument(skip_all, fields(request_id = msg.request_id))]
    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Handling DeclareMiningJobSuccess from JDS");

        let Some(last_declare_job) = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&msg.request_id).cloned())
        else {
            warn!(
                "No last_declare_job found for request_id={}",
                msg.request_id
            );
            return Ok(());
        };

        let Some(custom_job) = last_declare_job.custom_job else {
            debug!("No custom_job present for request_id={}", msg.request_id);
            return Ok(());
        };

        debug!("Forwarding SetCustomMiningJob upstream");
        let message = AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job));
        let frame: StdFrame = message
            .try_into()
            .map_err(|e| Error::External(Box::new(JDCError::Parser(e))))?;

        self.channel_manager_channel
            .upstream_sender
            .send(frame.into())
            .await
            .map_err(|e| Error::External(Box::new(JDCError::ChannelErrorSender)))?;

        info!("Successfully sent SetCustomMiningJob upstream");
        Ok(())
    }

    #[instrument(name = "provide_missing_transaction", skip_all, fields(request_id = msg.request_id))]
    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions<'_>,
    ) -> Result<(), Error> {
        info!("Handling ProvideMissingTransactions from JDS");

        let tx_store_entry = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&msg.request_id).cloned());

        let Some(entry) = tx_store_entry else {
            warn!(
                "No transaction list found for request_id={}",
                msg.request_id
            );
            return Ok(());
        };

        let full_tx_list: Vec<B016M> = entry
            .tx_list
            .iter()
            .map(|raw| B016M::from_vec_unchecked(raw.clone()))
            .collect();

        let unknown_tx_position_list: Vec<u16> = msg.unknown_tx_position_list.clone().into_inner();

        let unknown_positions: Vec<u16> = msg.unknown_tx_position_list.into_inner();
        debug!(
            total_known = full_tx_list.len(),
            unknown_positions = unknown_positions.len(),
            "Resolving missing transactions"
        );

        let missing_txns: Vec<B016M> = unknown_positions
            .iter()
            .filter_map(|&pos| full_tx_list.get(pos as usize).cloned())
            .collect();

        if missing_txns.is_empty() {
            warn!(
                "No matching transactions found for request_id={}",
                msg.request_id
            );
        }

        let response = ProvideMissingTransactionsSuccess {
            request_id: msg.request_id,
            transaction_list: binary_sv2::Seq064K::new(missing_txns)
                .map_err(|e| Error::External(Box::new(JDCError::BinarySv2(e))))?,
        };
        let frame: StdFrame =
            AnyMessage::JobDeclaration(JobDeclaration::ProvideMissingTransactionsSuccess(response))
                .try_into()
                .map_err(|e| Error::External(Box::new(JDCError::Parser(e))))?;

        self.channel_manager_channel
            .jd_sender
            .send(frame.into())
            .await
            .map_err(|e| Error::External(Box::new(JDCError::ChannelErrorSender)))?;
        Ok(())
    }
}
