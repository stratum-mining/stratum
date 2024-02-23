use binary_sv2::ShortTxId;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseClientJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess, SubmitSolutionJd,
    },
    parsers::JobDeclaration,
};
use std::{convert::TryInto, io::Cursor};
use stratum_common::bitcoin::Transaction;
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::{errors::Error, parsers::PoolMessages as AllMessages};
use stratum_common::bitcoin::consensus::Decodable;

use crate::mempool::{self, rpc_client::RpcApi};

use super::signed_token;

use super::JobDeclaratorDownstream;

impl JobDeclaratorDownstream {
    fn verify_job(&mut self, message: &DeclareMiningJob) -> bool {
        // Convert token from B0255 to u32
        let four_byte_array: [u8; 4] = message
            .mining_job_token
            .clone()
            .to_vec()
            .as_slice()
            .try_into()
            .unwrap();
        let token_u32 = u32::from_le_bytes(four_byte_array);
        // TODO Function to implement, it must be checked if the requested job has:
        // 1. right coinbase
        // 2. right version field
        // 3. right prev-hash
        // 4. right nbits
        self.token_to_job_map.contains_key(&(token_u32))
    }
}

impl ParseClientJobDeclarationMessages for JobDeclaratorDownstream {
    fn handle_allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error> {
        let token = self.tokens.next();
        self.token_to_job_map.insert(token, None);
        let message_success = AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: token.to_le_bytes().to_vec().try_into().unwrap(),
            coinbase_output_max_additional_size: 100,
            async_mining_allowed: true,
            coinbase_output: self.coinbase_output.clone().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::AllocateMiningJobTokenSuccess(message_success);
        println!(
            "Sending AllocateMiningJobTokenSuccess to proxy {:?}",
            message_enum
        );
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_declare_mining_job(&mut self, message: DeclareMiningJob) -> Result<SendTo, Error> {
        self.tx_hash_list_hash = Some(message.tx_hash_list_hash.clone().into_static());
        if self.verify_job(&message) {
            let short_hash_list: Vec<ShortTxId> = message
                .tx_short_hash_list
                .inner_as_ref()
                .iter()
                .map(|x| x.to_vec().try_into().unwrap())
                .collect();
            let nonce = message.tx_short_hash_nonce;
            // TODO return None when we have a collision handle that case as weel
            let short_id_mempool = self
                .mempool
                .safe_lock(|x| x.to_short_ids(nonce))
                .unwrap()
                .unwrap();
            let mut txs_in_job = vec![];
            let mut missing_txs = vec![];

            for (i, sid) in short_hash_list.iter().enumerate() {
                let sid_: [u8; 6] = sid.to_vec().try_into().unwrap();
                match short_id_mempool.get(&sid_) {
                    Some(tx_data) => match &tx_data.tx {
                        Some(tx) => txs_in_job.push(tx.clone()),
                        None => {
                            let new_tx_data = self
                                .mempool
                                .safe_lock(|x| x.get_client())
                                .unwrap()
                                .unwrap()
                                .get_raw_transaction(&tx_data.id.to_string(), None);
                            if let Ok(new_tx) = new_tx_data {
                                mempool::JDsMempool::add_tx_data_to_mempool(
                                    self.mempool.clone(),
                                    tx_data.clone().id,
                                    Some(new_tx.clone()),
                                );
                                txs_in_job.push(new_tx);
                            }
                        }
                    },
                    None => missing_txs.push(i as u16),
                }
            }
            self.declared_mining_job = Some((
                message.clone().into_static(),
                txs_in_job,
                missing_txs.clone(),
            ));

            if missing_txs.is_empty() {
                let message_success = DeclareMiningJobSuccess {
                    request_id: message.request_id,
                    new_mining_job_token: signed_token(
                        message.tx_hash_list_hash.clone(),
                        &self.public_key.clone(),
                        &self.private_key.clone(),
                    ),
                };
                let message_enum_success = JobDeclaration::DeclareMiningJobSuccess(message_success);
                Ok(SendTo::Respond(message_enum_success))
            } else {
                let message_provide_missing_transactions = ProvideMissingTransactions {
                    request_id: message.request_id,
                    unknown_tx_position_list: missing_txs.into(),
                };
                let message_enum_provide_missing_transactions =
                    JobDeclaration::ProvideMissingTransactions(
                        message_provide_missing_transactions,
                    );
                Ok(SendTo_::Respond(message_enum_provide_missing_transactions))
            }
        } else {
            let message_error = DeclareMiningJobError {
                request_id: message.request_id,
                error_code: Vec::new().try_into().unwrap(),
                error_details: Vec::new().try_into().unwrap(),
            };
            let message_enum_error = JobDeclaration::DeclareMiningJobError(message_error);
            Ok(SendTo::Respond(message_enum_error))
        }
    }

    fn handle_identify_transactions_success(
        &mut self,
        _message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        match &mut self.declared_mining_job {
            Some((_, ref mut transactions, missing_indexes)) => {
                for (i, tx) in message.transaction_list.inner_as_ref().iter().enumerate() {
                    let mut cursor = Cursor::new(tx);
                    let tx = Transaction::consensus_decode_from_finite_reader(&mut cursor)
                        .expect("Invalid tx data from downstream");
                    let index =
                        *missing_indexes
                            .get(i)
                            .ok_or(Error::LogicErrorMessage(Box::new(
                                AllMessages::JobDeclaration(
                                    JobDeclaration::ProvideMissingTransactionsSuccess(
                                        message.clone().into_static(),
                                    ),
                                ),
                            )))? as usize;
                    transactions.insert(index, tx);
                }
                // TODO check it
                let tx_hash_list_hash = self.tx_hash_list_hash.clone().unwrap().into_static();
                let message_success = DeclareMiningJobSuccess {
                    request_id: message.request_id,
                    new_mining_job_token: signed_token(
                        tx_hash_list_hash,
                        &self.public_key.clone(),
                        &self.private_key.clone(),
                    ),
                };
                let message_enum_success = JobDeclaration::DeclareMiningJobSuccess(message_success);
                Ok(SendTo::Respond(message_enum_success))
            }
            None => Err(Error::LogicErrorMessage(Box::new(
                AllMessages::JobDeclaration(JobDeclaration::ProvideMissingTransactionsSuccess(
                    message.clone().into_static(),
                )),
            ))),
        }
    }

    fn handle_submit_solution(&mut self, message: SubmitSolutionJd<'_>) -> Result<SendTo, Error> {
        let m = JobDeclaration::SubmitSolution(message.clone().into_static());

        Ok(SendTo::None(Some(m)))
    }
}
