use std::convert::TryInto;

use binary_sv2::ShortTxId;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseClientJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactionsSuccess, ProvideMissingTransactions,
    },
    mining_sv2::{SubmitSharesError, SubmitSharesExtended, SubmitSharesSuccess},
    parsers::JobDeclaration,
};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::errors::Error;

use crate::lib::job_declarator::signed_token;

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
        if self.verify_job(&message) {
            let short_hash_list: Vec<ShortTxId> = message
                .tx_short_hash_list
                .inner_as_ref()
                .iter()
                .map(|x| x.to_vec().try_into().unwrap())
                .collect();
            let nonce = message.tx_short_hash_nonce;
            let mempool = self.mempool.safe_lock(|x| x.clone()).unwrap();

            let mut unidentified_txs: Vec<u16> = Vec::new();
            let mut identified_txs: Vec<(
                stratum_common::bitcoin::Txid,
                stratum_common::bitcoin::Transaction,
            )> = Vec::new();
            //TODO use references insted cloning!!!!
            for i in 0..short_hash_list.len() {
                let tx_short_id = short_hash_list.get(i).unwrap();
                match mempool.verify_short_id(tx_short_id, nonce) {
                    Some(tx_with_id) => identified_txs.push(tx_with_id.clone()),
                    None => unidentified_txs.push(i.try_into().unwrap()),
                }
            }

            // TODO
            if !unidentified_txs.is_empty() {
                let message_provide_missing_txs = ProvideMissingTransactions {
                    request_id: message.request_id,
                    unknown_tx_position_list: {
                       unidentified_txs.clone().try_into().unwrap() 
                    },
                };
            };

            self.identified_txs = Some(identified_txs);
            self.unidentified_txs_indexes = unidentified_txs;
            let message_success = DeclareMiningJobSuccess {
                request_id: message.request_id,
                new_mining_job_token: signed_token(
                    message.tx_hash_list_hash,
                    &self.public_key.clone(),
                    &self.private_key.clone(),
                ),
            };
            let message_enum_success = JobDeclaration::DeclareMiningJobSuccess(message_success);
            Ok(SendTo::Respond(message_enum_success))
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
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        let message_success = IdentifyTransactionsSuccess {
            request_id: message.request_id,
            tx_data_hashes: Vec::new().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::IdentifyTransactionsSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error> {
        let message_success = ProvideMissingTransactionsSuccess {
            request_id: message.request_id,
            transaction_list: Vec::new().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::ProvideMissingTransactionsSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_submit_shares_extended(
        &mut self,
        message: SubmitSharesExtended,
    ) -> Result<SendTo, Error> {
        //TODO: implement logic for success or error

        let message_success = SubmitSharesSuccess {
            channel_id: message.channel_id,
            last_sequence_number: 0,
            new_submits_accepted_count: 0,
            new_shares_sum: 0,
        };
        let _message_enum = JobDeclaration::SubmitSharesSuccess(message_success);

        let message_error = SubmitSharesError {
            channel_id: message.channel_id,
            sequence_number: 0,
            error_code: Vec::new().try_into().unwrap(),
        };
        let message_enum = JobDeclaration::SubmitSharesError(message_error);
        Ok(SendTo::Respond(message_enum))
    }

    fn handle_submit_shares_success(
        &mut self,
        _message: SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, Error> {
        todo!()
    }

    fn handle_submit_shares_error(
        &mut self,
        _message: SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, Error> {
        todo!()
    }
}
