use binary_sv2::ShortTxId;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseClientJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess, SubmitSolutionJd,
    },
    parsers::JobDeclaration,
    utils::Mutex,
};
use std::{convert::TryInto, io::Cursor, sync::Arc};
use stratum_common::bitcoin::{Transaction, Txid};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use crate::mempool::JDsMempool;

use super::{signed_token, TransactionState};
use roles_logic_sv2::{errors::Error, parsers::PoolMessages as AllMessages};
use stratum_common::bitcoin::consensus::Decodable;
use tracing::{debug, info};

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
            async_mining_allowed: self.async_mining_allowed,
            coinbase_output: self.coinbase_output.clone().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::AllocateMiningJobTokenSuccess(message_success);
        info!(
            "Sending AllocateMiningJobTokenSuccess to proxy {:?}",
            message_enum
        );
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_declare_mining_job(&mut self, message: DeclareMiningJob) -> Result<SendTo, Error> {
        if let Some(old_mining_job) = self.declared_mining_job.0.take() {
            clear_declared_mining_job(old_mining_job, &message, self.mempool.clone())?;
        }

        // the transactions that are present in the mempool are stored here, that is sent to the
        // mempool which use the rpc client to retrieve the whole data for each transaction.
        // The unknown transactions is a vector that contains the transactions that are not in the
        // jds mempool, and will be non-empty in the ProvideMissingTransactionsSuccess message
        let mut known_transactions: Vec<Txid> = vec![];
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
            let mut transactions_with_state =
                vec![TransactionState::Missing; short_hash_list.len()];
            let mut missing_txs: Vec<u16> = Vec::new();

            for (i, sid) in short_hash_list.iter().enumerate() {
                let sid_: [u8; 6] = sid.to_vec().try_into().unwrap();
                match short_id_mempool.get(&sid_) {
                    Some(tx_data) => {
                        transactions_with_state[i] = TransactionState::PresentInMempool(tx_data.id);
                        known_transactions.push(tx_data.id);
                    }
                    None => {
                        transactions_with_state[i] = TransactionState::Missing;
                        missing_txs.push(i as u16);
                    }
                }
            }
            self.declared_mining_job = (
                Some(message.clone().into_static()),
                transactions_with_state,
                missing_txs.clone(),
            );
            // here we send the transactions that we want to be stored in jds mempool with full data

            self.add_txs_to_mempool
                .add_txs_to_mempool_inner
                .known_transactions
                .append(&mut known_transactions);

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
                Ok(SendTo::Respond(message_enum_provide_missing_transactions))
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
        let (declared_mining_job, ref mut transactions_with_state, missing_indexes) =
            &mut self.declared_mining_job;
        let mut unknown_transactions: Vec<Transaction> = vec![];
        match declared_mining_job {
            Some(declared_job) => {
                let id = declared_job.request_id;
                // check request_id in order to ignore old ProvideMissingTransactionsSuccess (see
                // issue #860)
                if id == message.request_id {
                    for (i, tx) in message.transaction_list.inner_as_ref().iter().enumerate() {
                        let mut cursor = Cursor::new(tx);
                        let transaction =
                            Transaction::consensus_decode_from_finite_reader(&mut cursor)
                                .map_err(|e| Error::TxDecodingError(e.to_string()))?;
                        Vec::push(&mut unknown_transactions, transaction.clone());
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
                        // insert the missing transactions in the mempool
                        transactions_with_state[index] =
                            TransactionState::PresentInMempool(transaction.txid());
                    }
                    self.add_txs_to_mempool
                        .add_txs_to_mempool_inner
                        .unknown_transactions
                        .append(&mut unknown_transactions);
                    // if there still a missing transaction return an error
                    for tx_with_state in transactions_with_state {
                        match tx_with_state {
                            TransactionState::PresentInMempool(_) => continue,
                            TransactionState::Missing => return Err(Error::JDSMissingTransactions),
                        }
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
                    let message_enum_success =
                        JobDeclaration::DeclareMiningJobSuccess(message_success);
                    return Ok(SendTo::Respond(message_enum_success));
                }
            }
            None => return Err(Error::NoValidJob),
        }
        Ok(SendTo::None(None))
    }

    fn handle_submit_solution(&mut self, message: SubmitSolutionJd<'_>) -> Result<SendTo, Error> {
        let m = JobDeclaration::SubmitSolution(message.clone().into_static());

        Ok(SendTo::None(Some(m)))
    }
}

fn clear_declared_mining_job(
    old_mining_job: DeclareMiningJob,
    new_mining_job: &DeclareMiningJob,
    mempool: Arc<Mutex<JDsMempool>>,
) -> Result<(), Error> {
    let old_transactions = old_mining_job.tx_short_hash_list.inner_as_ref();
    let new_transactions = new_mining_job.tx_short_hash_list.inner_as_ref();

    if old_transactions.is_empty() {
        info!("No transactions to remove from mempool");
        return Ok(());
    }

    let nonce = old_mining_job.tx_short_hash_nonce;

    let result = mempool
        .safe_lock(|mempool_| -> Result<(), Error> {
            let short_ids_map = mempool_
                .to_short_ids(nonce)
                .ok_or(Error::JDSMissingTransactions)?;

            for short_id in old_transactions
                .iter()
                .filter(|&id| !new_transactions.contains(id))
            {
                if let Some(transaction_with_hash) = short_ids_map.get(*short_id) {
                    let txid = transaction_with_hash.id;
                    match mempool_.mempool.get_mut(&txid) {
                        Some(Some((_transaction, counter))) => {
                            if *counter > 1 {
                                *counter -= 1;
                                debug!(
                                    "Fat transaction {:?} counter decremented; job id {:?} dropped",
                                    txid, old_mining_job.request_id
                                );
                            } else {
                                mempool_.mempool.remove(&txid);
                                debug!(
                                    "Fat transaction {:?} with job id {:?} removed from mempool",
                                    txid, old_mining_job.request_id
                                );
                            }
                        }
                        Some(None) => debug!(
                            "Thin transaction {:?} with job id {:?} removed from mempool",
                            txid, old_mining_job.request_id
                        ),
                        None => {}
                    }
                } else {
                    debug!(
                        "Transaction with short id {:?} not found in mempool for old jobs",
                        short_id
                    );
                }
            }
            Ok(())
        })
        .map_err(|e| Error::PoisonLock(e.to_string()))?;

    result.map_err(|err| Error::PoisonLock(err.to_string()))
}
