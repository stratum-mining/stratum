use crate::lib::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::SetCoinbase,
    parsers::JobNegotiation,
};
use tracing::info;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn handle_set_coinbase(&mut self, message: SetCoinbase) -> Result<SendTo, Error> {
        info!(
            "Received allocate mining job token success message: {:?}",
            message
        );
        Ok(SendTo::None(Some(JobNegotiation::SetCoinbase(
            message.into_static(),
        ))))
    }
}
