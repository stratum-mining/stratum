use crate::{parsers::JobNegotiation, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobNegotiation, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_negotiation_sv2::CoinbaseOutputDataSize;

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
            Ok(JobNegotiation::CoinbaseOutputDataSize(message)) => self_
                .safe_lock(|x| x.handle_message_coinbase_output_data_size(message))
                .unwrap(),
            Err(e) => Err(e),
        }
    }
    fn handle_message_coinbase_output_data_size(
        &mut self,
        message: CoinbaseOutputDataSize,
    ) -> Result<SendTo, Error>;
}
