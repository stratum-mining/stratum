use std::convert::TryInto;

use roles_logic_sv2::{
    handlers::{job_declaration::ParseClientJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactionsSuccess,
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
        // Token is saved in JobDeclaratorDownstream as u32
        self.token_to_job_map.insert(token, None);
        let message_success = AllocateMiningJobTokenSuccess {
            request_id: message.request_id,
            // From u32 token is transformed into B0255 in
            // AllocateMiningJobTokenSuccess message
            mining_job_token: token.to_le_bytes().to_vec().try_into().unwrap(),
            // Mock value of coinbase_max_additional_size. Must be changed
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

    fn handle_message_job_declaration(
        self_: std::sync::Arc<roles_logic_sv2::utils::Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobDeclaration::AllocateMiningJobToken(message)) => {
                println!("Allocate mining job token message sent to Proxy");
                self_
                    .safe_lock(|x| x.handle_allocate_mining_job_token(message))
                    .unwrap()
            }
            Ok(JobDeclaration::DeclareMiningJob(message)) => self_
                .safe_lock(|x| x.handle_declare_mining_job(message))
                .unwrap(),
            Ok(JobDeclaration::IdentifyTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.handle_identify_transactions_success(message))
                .unwrap(),
            Ok(JobDeclaration::ProvideMissingTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.handle_provide_missing_transactions_success(message))
                .unwrap(),
            Ok(JobDeclaration::SubmitSharesExtended(message)) => self_
                .safe_lock(|x| x.handle_submit_shares_extended(message))
                .unwrap(),
            Ok(JobDeclaration::AllocateMiningJobTokenSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x51", 16).unwrap(),
            )),
            Ok(JobDeclaration::DeclareMiningJobSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x58", 16).unwrap(),
            )),
            Ok(JobDeclaration::DeclareMiningJobError(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x59", 16).unwrap(),
            )),
            Ok(JobDeclaration::IdentifyTransactions(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x54", 16).unwrap(),
            )),
            Ok(JobDeclaration::ProvideMissingTransactions(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x55", 16).unwrap(),
            )),
            Ok(JobDeclaration::SubmitSharesSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x1c", 16).unwrap(),
            )),
            Ok(JobDeclaration::SubmitSharesError(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("0x1d", 16).unwrap(),
            )),
            Err(e) => Err(e),
        }
    }
}
