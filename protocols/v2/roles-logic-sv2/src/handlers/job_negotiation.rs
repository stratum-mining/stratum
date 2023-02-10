use crate::{parsers::JobNegotiation, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_negotiation_sv2::SetCoinbase;

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
            Ok(JobNegotiation::SetCoinbase(message)) => self_
                .safe_lock(|x| x.handle_set_coinbase(message))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Err(e) => Err(e),
        }
    }
    /// Construct a new coinbase transaction based on the message
    fn handle_set_coinbase(&mut self, message: SetCoinbase) -> Result<SendTo, Error>;
}
