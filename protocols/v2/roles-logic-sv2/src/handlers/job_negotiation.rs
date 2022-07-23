use crate::{parsers::JobNegotiation, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_negotiation_sv2::{
    AllocateMiningJobToken, AllocateMiningJobTokenSuccess, CommitMiningJob, CommitMiningJobError,
    CommitMiningJobSuccess, IdentifyTransactions, IdentifyTransactionsSuccess,
    ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
};

pub trait ParseServerJobNegotiationMessages
where
    Self: Sized,
{
    fn handle_message_job_negotiation(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobTokenSuccess(message)) => self_
                .safe_lock(|x| x.allocate_mining_job_token_success(message))
                .unwrap(),
            Ok(JobNegotiation::CommitMiningJobSuccess(message)) => self_
                .safe_lock(|x| x.commit_mining_job_success(message))
                .unwrap(),
            Ok(JobNegotiation::CommitMiningJobError(message)) => self_
                .safe_lock(|x| x.commit_mining_job_error(message))
                .unwrap(),
            Ok(JobNegotiation::IdentifyTransactions(message)) => self_
                .safe_lock(|x| x.identify_transactions(message))
                .unwrap(),
            Ok(JobNegotiation::ProvideMissingTransactions(message)) => self_
                .safe_lock(|x| x.provide_missing_transactions(message))
                .unwrap(),
            Ok(JobNegotiation::AllocateMiningJobToken(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJob(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::IdentifyTransactionsSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::ProvideMissingTransactionsSuccess(_)) => {
                Err(Error::UnexpectedMessage)
            }
            Err(e) => Err(e),
        }
    }
    fn allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error>;
    fn commit_mining_job_success(
        &mut self,
        message: CommitMiningJobSuccess,
    ) -> Result<SendTo, Error>;
    fn commit_mining_job_error(&mut self, message: CommitMiningJobError) -> Result<SendTo, Error>;
    fn identify_transactions(&mut self, message: IdentifyTransactions) -> Result<SendTo, Error>;
    fn provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error>;
}

pub trait ParseClientJobNegotiationMessages
where
    Self: Sized,
{
    fn handle_message_job_negotiation(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobToken(message)) => self_
                .safe_lock(|x| x.allocate_mining_job_token(message))
                .unwrap(),
            Ok(JobNegotiation::CommitMiningJob(message)) => {
                self_.safe_lock(|x| x.commit_mining_job(message)).unwrap()
            }
            Ok(JobNegotiation::IdentifyTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.identify_transactions_success(message))
                .unwrap(),
            Ok(JobNegotiation::ProvideMissingTransactionsSuccess(message)) => self_
                .safe_lock(|x| x.provide_missing_transactions_success(message))
                .unwrap(),
            Ok(JobNegotiation::AllocateMiningJobTokenSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJobSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::CommitMiningJobError(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::IdentifyTransactions(_)) => Err(Error::UnexpectedMessage),
            Ok(JobNegotiation::ProvideMissingTransactions(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }
    fn allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error>;
    fn commit_mining_job(&mut self, message: CommitMiningJob) -> Result<SendTo, Error>;
    fn identify_transactions_success(
        &mut self,
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error>;
    fn provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error>;
}
