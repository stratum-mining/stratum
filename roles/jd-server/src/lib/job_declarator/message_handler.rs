use std::{convert::TryInto, io::Cursor, sync::Arc};
use stratum_common::roles_logic_sv2::{
    bitcoin::{
        consensus::Decodable as BitcoinDecodable,
        hashes::{sha256d, Hash},
        Transaction, Txid,
    },
    codec_sv2::binary_sv2::{Decodable, Serialize, U256},
    handlers::{job_declaration::ParseJobDeclarationMessagesFromDownstream, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, ProvideMissingTransactions,
        ProvideMissingTransactionsSuccess, PushSolution,
    },
    parsers_sv2::JobDeclaration,
    utils::Mutex,
};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use crate::mempool::JDsMempool;

use super::{signed_token, TransactionState};
use stratum_common::roles_logic_sv2::{errors::Error, parsers_sv2::AnyMessage as AllMessages};
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

impl ParseJobDeclarationMessagesFromDownstream for JobDeclaratorDownstream {
    fn handle_allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `AllocateMiningJobToken` with id: {}",
            message.request_id
        );
        debug!("`AllocateMiningJobToken`: {:?}", message.request_id);
        let token = self.tokens.next();
        self.token_to_job_map.insert(token, None);
        let message_success = AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            mining_job_token: token.to_le_bytes().to_vec().try_into().unwrap(),
            coinbase_outputs: self.coinbase_output.clone().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::AllocateMiningJobTokenSuccess(message_success);
        info!(
            "Sending AllocateMiningJobTokenSuccess to proxy {}",
            message_enum
        );
        Ok(SendTo::Respond(message_enum))
    }

    // Transactions that are present in the mempool are stored here, that is sent to the
    // mempool which use the rpc client to retrieve the whole data for each transaction.
    // The unknown transactions is a vector that contains the transactions that are not in the
    // jds mempool, and will be non-empty in the ProvideMissingTransactionsSuccess message
    fn handle_declare_mining_job(&mut self, message: DeclareMiningJob) -> Result<SendTo, Error> {
        info!(
            "Received `DeclareMiningJob` with id: {}",
            message.request_id
        );
        debug!("`DeclareMiningJob`: {}", message);
        if let Some(old_mining_job) = self.declared_mining_job.0.take() {
            clear_declared_mining_job(old_mining_job, &message, self.mempool.clone())?;
        }
        let mut known_transactions: Vec<Txid> = vec![];
        if self.verify_job(&message) {
            let txids = message.tx_ids_list.inner_as_ref();
            let mempool = self.mempool.safe_lock(|x| x.mempool.clone())?;
            let mut transactions_with_state = vec![TransactionState::Missing; txids.len()];
            let mut missing_txs: Vec<u16> = Vec::new();
            for (i, txid) in txids.iter().enumerate() {
                let hash = sha256d::Hash::from_slice(txid)?;
                let txid = Txid::from(hash);
                match mempool.contains_key(&txid) {
                    true => {
                        transactions_with_state[i] = TransactionState::PresentInMempool(txid);
                        known_transactions.push(txid);
                    }
                    false => {
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
            let mut full_token = [0u8; 255];
            message.mining_job_token.to_bytes(&mut full_token)?;
            let mining_job_token = &mut full_token[..32];
            if missing_txs.is_empty() {
                let message_success = DeclareMiningJobSuccess {
                    request_id: message.request_id,
                    new_mining_job_token: signed_token(
                        U256::from_bytes(mining_job_token)?,
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

    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `ProvideMissingTransactionsSuccess` with id: {}",
            message.request_id
        );
        debug!("`ProvideMissingTransactionsSuccess`: {}", message);
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
                            TransactionState::PresentInMempool(transaction.compute_txid());
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
                    let mut full_token = [0u8; 255];
                    declared_job
                        .mining_job_token
                        .clone()
                        .to_bytes(&mut full_token)?;
                    let mining_job_token = &mut full_token[..32];
                    let message_success = DeclareMiningJobSuccess {
                        request_id: message.request_id,
                        new_mining_job_token: signed_token(
                            U256::from_bytes(mining_job_token)?,
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

    fn handle_push_solution(&mut self, message: PushSolution<'_>) -> Result<SendTo, Error> {
        info!("Received PushSolution from JDC");
        debug!("`PushSolution`: {}", message);
        let m = JobDeclaration::PushSolution(message.clone().into_static());
        Ok(SendTo::None(Some(m)))
    }
}

fn clear_declared_mining_job(
    old_mining_job: DeclareMiningJob,
    new_mining_job: &DeclareMiningJob,
    mempool: Arc<Mutex<JDsMempool>>,
) -> Result<(), Error> {
    let old_transactions = old_mining_job.tx_ids_list.inner_as_ref();
    let new_transactions = new_mining_job.tx_ids_list.inner_as_ref();

    if old_transactions.is_empty() {
        info!("No transactions to remove from mempool");
        return Ok(());
    }

    let result = mempool.safe_lock(|mempool_| -> Result<(), Error> {
        let mempool_txs = mempool_.mempool.clone();

        for old_txid in old_transactions
            .iter()
            .filter(|&id| !new_transactions.contains(id))
        {
            if let Some(tx) = mempool_txs.get(*old_txid) {
                if let Some((transaction, _)) = tx.as_ref() {
                    let txid = transaction.compute_txid();
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
                    debug!("Transaction with id {:?} is None in mempool", old_txid);
                }
            } else {
                debug!(
                    "Transaction with id {:?} not found in mempool for old jobs",
                    old_txid
                );
            }
        }
        Ok(())
    })?;

    result.map_err(|err| Error::PoisonLock(err.to_string()))
}
