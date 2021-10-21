use messages_sv2::CommonMessages::{UpstreamCommon, DownstreamCommon, CommonMessages};
use codec_sv2::Frame;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use async_channel::{Receiver, SendError, Sender};

pub type Message = CommonMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub struct Sv2UpstreamConnection {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
}

impl UpstreamCommon for Sv2UpstreamConnection {
}

pub struct Sv2DownstreamConnection {
}
