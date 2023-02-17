use crate::lib::job_negotiator::JobNegotiator;
use roles_logic_sv2::{
    handlers::{job_negotiation::ParseServerJobNegotiationMessages, SendTo_},
    job_negotiation_sv2::SetCoinbase,
    parsers::JobNegotiation,
};
use tracing::info;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use core::convert::TryInto;
use roles_logic_sv2::{bitcoin::TxOut, errors::Error};

impl ParseServerJobNegotiationMessages for JobNegotiator {
    fn handle_set_coinbase(&mut self, message: SetCoinbase) -> Result<SendTo, Error> {
        info!(
            "Received allocate mining job token success message: {:?}",
            message
        );
        let txout = TxOut {
            value: self.coinbase_reward_sat,
            script_pubkey: message.coinbase_tx_suffix.to_vec().try_into().unwrap(),
        };
        // TODO this should be a message of the JN protocol so the JN can communicate to the proxy
        // the coinbase out when they are not part of the same program
        self.last_coinbase_out = Some(vec![txout]);
        Ok(SendTo::None(Some(JobNegotiation::SetCoinbase(
            message.into_static(),
        ))))
    }
}
