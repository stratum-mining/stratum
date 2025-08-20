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
use tracing::info;

use crate::{channel_manager::ChannelManager, utils::StdFrame};

impl HandleJobDeclarationMessagesFromServerAsync for ChannelManager {
    async fn handle_allocate_mining_job_token_success(
        &mut self,
        msg: AllocateMiningJobTokenSuccess<'_>,
    ) -> Result<(), Error> {
        info!("Received allocate Mining token success from JDS");
        let coinbase_output = self.channel_manager_data.super_safe_lock(|data| {
            let value = data.coinbase_outputs == msg.coinbase_outputs.to_vec();
            data.coinbase_outputs = msg.coinbase_outputs.to_vec();
            data.allocate_tokens = Some(msg.clone().into_static());
            value
        });
        if !coinbase_output {
            let deserialized_jds_coinbase_outputs: Vec<TxOut> =
                bitcoin::consensus::deserialize(&msg.coinbase_outputs.to_vec())
                    .expect("Invalid coinbase output");

            let coinbase_output_max_additional_size: usize = deserialized_jds_coinbase_outputs
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

            let coinbase_output_max_additional_sigops =
                dummy_coinbase.total_sigop_cost(|_| None) as u16;

            let coinbase_output_data_size = AnyMessage::TemplateDistribution(
                TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
                    coinbase_output_max_additional_size: coinbase_output_max_additional_size as u32,
                    coinbase_output_max_additional_sigops,
                }),
            );

            let frame: StdFrame = coinbase_output_data_size.try_into().unwrap();
            self.channel_manager_channel
                .tp_sender
                .send(frame.into())
                .await;
        }

        Ok(())
    }

    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError<'_>,
    ) -> Result<(), Error> {
        // fallback
        info!("Received handle_declare_mining_job_error from JDS");
        Ok(())
    }

    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess<'_>,
    ) -> Result<(), Error> {
        // https://stratumprotocol.org/specification/06-Job-Declaration-Protocol/#641-setupconnection-flags-for-job-declaration-protocol
        info!("Received handle_declare_mining_job_success from JDS");
        let last_declare_job = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&msg.request_id).cloned());
        if let Some(last_declare_job) = last_declare_job {
            let custom_job = last_declare_job.custom_job;
            if let Some(custom_job) = custom_job {
                let message = AnyMessage::Mining(Mining::SetCustomMiningJob(custom_job));
                let frame: StdFrame = message.try_into().unwrap();

                self.channel_manager_channel
                    .upstream_sender
                    .send(frame.into())
                    .await;
            }
        }

        Ok(())
    }

    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions<'_>,
    ) -> Result<(), Error> {
        info!("Received handle_provide_missing_transactions from JDS");

        let tx_list = self
            .channel_manager_data
            .super_safe_lock(|data| data.last_declare_job_store.get(&msg.request_id).cloned());

        if tx_list.is_none() {
            // we should return error in this case. Story for another time,
            return Ok(());
        }

        let tx_list: Vec<binary_sv2::B016M> = tx_list
            .unwrap()
            .tx_list
            .iter()
            .map(|data| B016M::from_vec_unchecked(data.clone()))
            .collect();

        let unknown_tx_position_list: Vec<u16> = msg.unknown_tx_position_list.into_inner();

        let missing_transactions: Vec<binary_sv2::B016M> = unknown_tx_position_list
            .iter()
            .filter_map(|&pos| tx_list.get(pos as usize).cloned())
            .collect();
        let request_id = msg.request_id;
        let message_provide_missing_transactions = ProvideMissingTransactionsSuccess {
            request_id,
            transaction_list: binary_sv2::Seq064K::new(missing_transactions).unwrap(),
        };
        let message_enum = AnyMessage::JobDeclaration(
            JobDeclaration::ProvideMissingTransactionsSuccess(message_provide_missing_transactions),
        );

        let frame: StdFrame = message_enum.try_into().unwrap();
        self.channel_manager_channel
            .jd_sender
            .send(frame.into())
            .await;
        Ok(())
    }
}
