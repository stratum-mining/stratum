use crate::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    parsers::JobNegotiation,
};
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn handle_allocate_mining_job_token_sucess(
        &mut self,
        message: roles_logic_sv2::job_negotiation_sv2::AllocateMiningJobTokenSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, Error> {
        self.allocated_tokens.push(message.into_static());
        Ok(SendTo::None(None))
    }

    // We assume that server send success so we are already working on that job, notthing to do.
    fn handle_commit_mining_job_success(
        &mut self,
        message: roles_logic_sv2::job_negotiation_sv2::CommitMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, Error> {
        let message = JobNegotiation::CommitMiningJobSuccess(message.into_static());
        Ok(SendTo::None(Some(message)))
    }
}
