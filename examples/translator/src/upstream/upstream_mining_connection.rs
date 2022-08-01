use async_channel::{Receiver, SendError, Sender};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::PoolMessages;

pub type StdFrame = StandardSv2Frame<Message>;
pub type Message = PoolMessages<'static>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// A 1 to 1 connection with an upstream node that implements the mining (sub)protocol.
/// The upstream node it connects with is most typically a pool, but could also be another proxy.
#[derive(Debug, Clone)]
pub struct UpstreamMiningConnection {
    pub receiver: Receiver<EitherFrame>,
    pub sender: Sender<EitherFrame>,
}

impl UpstreamMiningConnection {
    pub async fn send(&mut self, sv2_frame: StdFrame) -> Result<(), SendError<EitherFrame>> {
        let either_frame = sv2_frame.into();
        match self.sender.send(either_frame).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
