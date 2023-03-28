use crate::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    parsers::JobNegotiation,
};
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn handle_allocate_mining_job_sucess(
        &mut self,
        message: roles_logic_sv2::job_negotiation_sv2::AllocateMiningJobTokenSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, Error> {
        Ok(SendTo::None(Some(
            JobNegotiation::AllocateMiningJobTokenSuccess(message.into_static()),
        )))
    }

    // We assume that server send success so we are already working on that job, notthing to do.
    fn handle_commit_mining_job_success(
        &mut self,
        _message: roles_logic_sv2::job_negotiation_sv2::CommitMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_negotiation::SendTo, Error> {
        Ok(SendTo::None(None))
    }
}
