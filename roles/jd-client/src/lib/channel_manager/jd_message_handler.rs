use stratum_common::roles_logic_sv2::{
    bitcoin::{
        self, absolute::LockTime, transaction::Version, OutPoint, ScriptBuf, Sequence, Transaction,
        TxIn, TxOut, Witness,
    },
    codec_sv2::binary_sv2::{self, Sv2DataType, B016M},
    handlers_sv2::HandleJobDeclarationMessagesFromServerAsync,
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
    },
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::CoinbaseOutputConstraints,
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::ChannelManager,
    error::JDCError,
    status::{State, Status},
    utils::{deserialize_coinbase_outputs, StdFrame},
};

impl HandleJobDeclarationMessagesFromServerAsync for ChannelManager {
    type Error = JDCError;

    // Handles a successful `AllocateMiningJobToken` response from the JDS.
    //
    // When the JDS confirms job token allocation:
    // - Updates the channel manager state with the newly issued token.
    // - Checks whether the JDS has provided updated coinbase outputs.
    //   - If outputs have changed, recalculates the corresponding size and sigops constraints.
    //   - Sends an updated `CoinbaseOutputConstraints` message to the Template Provider to ensure
    //     the new coinbase rules are enforced.
    // - If outputs are unchanged, skips recomputation and continues as normal.
    async fn handle_allocate_mining_job_token_success(
        &mut self,
        msg: AllocateMiningJobTokenSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let coinbase_changed = self.channel_manager_data.super_safe_lock(|data| {
            let changed = data.coinbase_outputs != msg.coinbase_outputs.to_vec();
            data.coinbase_outputs = msg.coinbase_outputs.to_vec();
            data.allocate_tokens = Some(msg.clone().into_static());
            changed
        });

        if coinbase_changed {
            info!("Coinbase outputs from JDS changed, recalculating constraints");
            let deserialized_jds_coinbase_outputs: Vec<TxOut> =
                bitcoin::consensus::deserialize(&msg.coinbase_outputs.to_vec())
                    .map_err(JDCError::BitcoinEncodeError)?;

            let max_additional_size: usize = deserialized_jds_coinbase_outputs
                .iter()
                .map(|o| o.size())
                .sum();

            // create a dummy coinbase transaction with the empty output
            // this is used to calculate the sigops of the coinbase output
            let dummy_coinbase = Transaction {
                version: Version::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::from(vec![vec![0; 32]]),
                }],
                output: deserialized_jds_coinbase_outputs,
            };

            let max_additional_sigops = dummy_coinbase.total_sigop_cost(|_| None) as u16;

            debug!(
                max_additional_size,
                max_additional_sigops, "Computed coinbase output constraints"
            );

            let coinbase_output_contraints_message = AnyMessage::TemplateDistribution(
                TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
                    coinbase_output_max_additional_size: max_additional_size as u32,
                    coinbase_output_max_additional_sigops: max_additional_sigops,
                }),
            );

            let frame: StdFrame = coinbase_output_contraints_message.try_into()?;

            self.channel_manager_channel
                .tp_sender
                .send(frame)
                .await
                .map_err(|_e| JDCError::ChannelErrorSender)?;

            info!("Sent updated CoinbaseOutputConstraints to TP channel");
        } else {
            debug!("Coinbase outputs unchanged, skipping constraints update");
        }

        Ok(())
    }

    // Handles a `DeclareMiningJobError` response from the JDS.
    //
    // Receiving this error is treated as a malicious or invalid upstream behavior,
    // since it indicates the JDS has rejected a declared mining job request.
    //
    // Upon receiving it:
    // - Triggers the fallback mechanism by signaling a shutdown through the status channel, causing
    //   the Job Declarator Client to enter `JobDeclaratorShutdownFallback`.
    //
    // This ensures that the system does not continue relying on a potentially
    // untrustworthy or misbehaving JDS, and instead fails over to a safer state.
    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        warn!("⚠️ JDS refused the declared job with a DeclareMiningJobError ❌. Starting fallback mechanism.");
        self.channel_manager_channel
            .status_sender
            .send(Status {
                state: State::JobDeclaratorShutdownFallback(JDCError::Shutdown),
            })
            .await
            .map_err(|_e| JDCError::ChannelErrorSender)?;

        Ok(())
    }

    // Handles a `DeclareMiningJobSuccess` message from the JDS.
    //
    // Receiving this message means the JDS has accepted the declared mining job,
    // giving us the green light to propagate it upstream.
    //
    // The steps are:
    // 1. Look up the last declared job using the `request_id`.
    // 2. Validate that a `prevhash` exists and retrieve job details.
    // 3. Use the job factory to create a new `SetCustomMiningJob` request, embedding the token
    //    provided by the JDS.
    // 4. Update the channel manager state with the newly created custom job.
    // 5. Send the `SetCustomMiningJob` message to the upstream, ensuring the job is now distributed
    //    across the mining network.
    //
    // If any required data (like `prevhash` or the last declared job) is missing,
    // this handler returns an error to prevent propagation of an incomplete job.
    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);

        let Some(last_declare_job) = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&msg.request_id).cloned())
        else {
            error!(
                "No last_declare_job found for request_id={}",
                msg.request_id
            );
            return Err(JDCError::LastDeclareJobNotFound(msg.request_id));
        };

        let Some(prevhash) = last_declare_job.prev_hash else {
            error!("Prevhash not found for request_id = {}", msg.request_id);
            return Err(JDCError::LastNewPrevhashNotFound);
        };

        let Some(custom_job) = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let job_factory = channel_manager_data.job_factory.as_mut()?;
                let upstream_channel = channel_manager_data.upstream_channel.as_ref()?;
                let output = deserialize_coinbase_outputs(&last_declare_job.coinbase_output);
                let custom_job = job_factory.new_custom_job(
                    upstream_channel.get_channel_id(),
                    msg.request_id,
                    msg.new_mining_job_token,
                    prevhash.into(),
                    last_declare_job.template,
                    output,
                );
                Some(custom_job)
            })
        else {
            return Err(JDCError::FailedToCreateCustomJob);
        };

        let custom_job = custom_job.map_err(|_e| JDCError::FailedToCreateCustomJob)?;

        self.channel_manager_data.super_safe_lock(|data| {
            if let Some(value) = data.last_declare_job_store.get_mut(&msg.request_id) {
                value.set_custom_mining_job = Some(custom_job.clone().into_static());
            }
        });

        let channel_id = custom_job.channel_id;

        debug!("Sending SetCustomMiningJob to the upstream with channel_id: {channel_id}");
        let message = AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job)).into_static();
        let frame: StdFrame = message.try_into()?;

        self.channel_manager_channel
            .upstream_sender
            .send(frame)
            .await
            .map_err(|_e| JDCError::ChannelErrorSender)?;

        info!("Successfully sent SetCustomMiningJob to the upstream with channel_id: {channel_id}");
        Ok(())
    }

    // Handles a `ProvideMissingTransactions` request from the JDS.
    //
    // The JDS provides a list of transaction positions it could not resolve.
    // We then:
    // - Retrieve the full transaction list for the given `request_id`.
    // - Identify which transactions are missing based on the provided positions.
    // - Collect and package those transactions into a `ProvideMissingTransactionsSuccess`.
    // - Send the response back to the JDS.
    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.request_id;

        info!("Received: {}", msg);

        let tx_store_entry = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&request_id).cloned());

        let Some(entry) = tx_store_entry else {
            warn!(
                "No transaction list found for request_id={}",
                msg.request_id
            );
            return Err(JDCError::LastDeclareJobNotFound(msg.request_id));
        };

        let full_tx_list: Vec<B016M> = entry
            .tx_list
            .iter()
            .map(|raw| B016M::from_vec_unchecked(raw.clone()))
            .collect();

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
            warn!("No matching transactions found for request_id={request_id}");
        }

        let response = ProvideMissingTransactionsSuccess {
            request_id: msg.request_id,
            transaction_list: binary_sv2::Seq064K::new(missing_txns)
                .map_err(JDCError::BinarySv2)?,
        };
        let frame: StdFrame =
            AnyMessage::JobDeclaration(JobDeclaration::ProvideMissingTransactionsSuccess(response))
                .try_into()?;

        self.channel_manager_channel
            .jd_sender
            .send(frame)
            .await
            .map_err(|_e| JDCError::ChannelErrorSender)?;

        info!("Successfully sent ProvideMissingTransactionsSuccess to the JDS with request_id: {request_id}");

        Ok(())
    }
}
