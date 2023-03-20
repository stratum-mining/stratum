use crate::{parsers::JobNegotiation, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_negotiation_sv2::*;

/// A trait implemented by a downstream to handle SV2 job negotiation messages.
pub trait ParseServerJobNegotiationMessages
where
    Self: Sized,
{
    /// Used to parse job negotiation message and route to the message's respected handler function
    fn handle_message_job_negotiation(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobTokenSuccess(message)) => self_
                .safe_lock(|x| x.handle_allocate_mining_job_sucess(message))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(JobNegotiation::CommitMiningJobSuccess(message)) => self_
                .safe_lock(|x| x.handle_commit_mining_job_success(message))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }
    fn handle_allocate_mining_job_sucess(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error>;
    fn handle_commit_mining_job_success(
        &mut self,
        message: CommitMiningJobSuccess,
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
        match (message_type, payload).try_into() {
            Ok(JobNegotiation::AllocateMiningJobToken(message)) => self_
                .safe_lock(|x| x.handle_allocate_mining_job(message))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(JobNegotiation::CommitMiningJob(message)) => self_
                .safe_lock(|x| x.handle_commit_mining_job(message))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }
    fn handle_allocate_mining_job(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error>;
    fn handle_commit_mining_job(&mut self, message: CommitMiningJob) -> Result<SendTo, Error>;
}
