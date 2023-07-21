use std::convert::TryInto;

use roles_logic_sv2::{
    handlers::{job_declaration::ParseClientJobDeclarationMessages, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactionsSuccess,
        ProvideMissingTransactionsSuccess,
    },
    parsers::JobDeclaration,
};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::errors::Error;

use super::JobDeclaratorDownstream;

impl JobDeclaratorDownstream {
    fn verify_job(&mut self, _message: &DeclareMiningJob) -> bool {
        // TODO: check if there is a token
        /* let is_token_allocated = self
        .token_to_job_map
        .contains_key(&message.mining_job_token); */
        // TODO Function to implement, it must be checked if the requested job has:
        // 1. right coinbase
        // 2. right version field
        // 3. right prev-hash
        // 4. right nbits
        // 5. a valid merkletpath
        // is_token_allocated
        // TODO:use is_token_allocated for this bool
        true
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
            mining_job_token: Vec::new().try_into().unwrap(),
            coinbase_output_max_additional_size: 0,
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
                new_mining_job_token: message.mining_job_token.into_static(),
            };
            let message_enum_success = JobDeclaration::DeclareMiningJobSuccess(message_success);
            // TODO: token map
            /* self.token_to_job_map
            .insert(message.mining_job_token, Some(message.into())); */
            println!(
                "Declare mining job was a success: {:?}",
                message_enum_success
            );
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
            Ok(JobDeclaration::AllocateMiningJobTokenSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("51", 16).unwrap(),
            )),
            Ok(JobDeclaration::DeclareMiningJobSuccess(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("58", 16).unwrap(),
            )),
            Ok(JobDeclaration::DeclareMiningJobError(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("59", 16).unwrap(),
            )),
            Ok(JobDeclaration::IdentifyTransactions(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("60", 16).unwrap(),
            )),
            Ok(JobDeclaration::ProvideMissingTransactions(_)) => Err(Error::UnexpectedMessage(
                u8::from_str_radix("62", 16).unwrap(),
            )),
            Err(e) => Err(e),
        }
    }
}
