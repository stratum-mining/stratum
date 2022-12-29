use crate::lib::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::CoinbaseOutputDataSize,
    parsers::JobNegotiation,
};
use tracing::info;
pub type SendTo = SendTo_<JobNegotiation, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn handle_message_coinbase_output_data_size(
        &mut self,
        message: CoinbaseOutputDataSize,
    ) -> Result<SendTo, Error> {
        info!(
            "Received allocate mining job token success message: {:?}\n",
            message
        );
        Ok(SendTo::None(Some(JobNegotiation::CoinbaseOutputDataSize(
            message,
        ))))
    }
}
