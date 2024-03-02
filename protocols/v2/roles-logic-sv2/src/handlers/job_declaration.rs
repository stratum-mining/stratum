use crate::{parsers::JobDeclaration, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_declaration_sv2::*;
use tracing::{debug, error, info, trace};

/// A trait implemented by a downstream to handle SV2 job declaration messages.
pub trait ParseServerJobDeclarationMessages
where
    Self: Sized,
{
    /// Used to parse job declaration message and route to the message's respected handler function
    fn handle_message_job_declaration(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_job_declaration_deserialized(self_, (message_type, payload).try_into())
    }

    fn handle_message_job_declaration_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<JobDeclaration<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(JobDeclaration::AllocateMiningJobTokenSuccess(message)) => {
                debug!(
                    "Received AllocateMiningJobTokenSuccess with id: {}",
                    message.request_id
                );
                trace!("AllocateMiningJobTokenSuccess: {:?}", message.request_id);
                self_
                    .safe_lock(|x| x.handle_allocate_mining_job_token_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJobSuccess(message)) => {
                info!(
                    "Received DeclareMiningJobSuccess with id {}",
                    message.request_id
                );
                debug!("DeclareMiningJobSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJobError(message)) => {
                error!(
                    "Received DeclareMiningJobError, error code: {}",
                    std::str::from_utf8(message.error_code.as_ref())
                        .unwrap_or("unknown error code")
                );
                debug!("DeclareMiningJobSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job_error(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::IdentifyTransactions(message)) => {
                info!(
                    "Received IdentifyTransactions with id: {}",
                    message.request_id
                );
                debug!("IdentifyTransactions: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_identify_transactions(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::ProvideMissingTransactions(message)) => {
                info!(
                    "Received ProvideMissingTransactions with id: {}",
                    message.request_id
                );
                debug!("ProvideMissingTransactions: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_provide_missing_transactions(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }
    /// When upstream send AllocateMiningJobTokenSuccess self should use the received token to
    /// negotiate the next job
    ///
    /// "[`job_declaration_sv2::AllocateMiningJobToken`]"
    fn handle_allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error>;

    // When upstream send DeclareMiningJobSuccess if the token is different from the one negotiated
    // self must use the new token to refer to the committed job
    fn handle_declare_mining_job_success(
        &mut self,
        message: DeclareMiningJobSuccess,
    ) -> Result<SendTo, Error>;

    // TODO: comment
    fn handle_declare_mining_job_error(
        &mut self,
        message: DeclareMiningJobError,
    ) -> Result<SendTo, Error>;

    // TODO: comment
    fn handle_identify_transactions(
        &mut self,
        message: IdentifyTransactions,
    ) -> Result<SendTo, Error>;

    // TODO: comment
    fn handle_provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error>;
}
pub trait ParseClientJobDeclarationMessages
where
    Self: Sized,
{
    fn handle_message_job_declaration(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_job_declaration_deserialized(self_, (message_type, payload).try_into())
    }

    fn handle_message_job_declaration_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<JobDeclaration<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(JobDeclaration::AllocateMiningJobToken(message)) => {
                debug!(
                    "Received AllocateMiningJobToken with id: {}",
                    message.request_id
                );
                trace!("AllocateMiningJobToken: {:?}", message.request_id);
                self_
                    .safe_lock(|x| x.handle_allocate_mining_job_token(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJob(message)) => {
                info!("Received DeclareMiningJob with id: {}", message.request_id);
                debug!("DeclareMiningJob: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::IdentifyTransactionsSuccess(message)) => {
                info!(
                    "Received IdentifyTransactionsSuccess with id: {}",
                    message.request_id
                );
                debug!("IdentifyTransactionsSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_identify_transactions_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::ProvideMissingTransactionsSuccess(message)) => {
                info!(
                    "Received ProvideMissingTransactionsSuccess with id: {}",
                    message.request_id
                );
                debug!("ProvideMissingTransactionsSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_provide_missing_transactions_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::SubmitSolution(message)) => {
                info!("Received SubmitSolution");
                debug!("SubmitSolution: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_submit_solution(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }

            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }

    fn handle_allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error>;

    fn handle_declare_mining_job(&mut self, message: DeclareMiningJob) -> Result<SendTo, Error>;

    fn handle_identify_transactions_success(
        &mut self,
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error>;

    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error>;
    fn handle_submit_solution(&mut self, message: SubmitSolutionJd) -> Result<SendTo, Error>;
}
