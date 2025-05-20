//! ## Upstream SV2 Connection Module
//!
//! Defines [`UpstreamConnection`], the structure responsible for managing the
//! communication channels with an upstream.

use super::{super::error::ProxyResult, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
/// On upstream, we have a sv2connection, so we use the connection from network helpers
/// use network_helpers::Connection;
/// this does the dirty work of reading byte by byte in the socket and puts them in a complete
/// Sv2Messages frame and when the message is ready then sends to our Upstream
/// sender_incoming + receiver_outgoing are in network_helpers::Connection
#[derive(Debug, Clone)]
pub struct UpstreamConnection {
    /// Receives messages from the SV2 Upstream role
    pub receiver: Receiver<EitherFrame>,
    /// Sends messages to the SV2 Upstream role
    pub sender: Sender<EitherFrame>,
}

impl UpstreamConnection {
    /// Send a SV2 message to the Upstream role
    pub async fn send(&mut self, sv2_frame: StdFrame) -> ProxyResult<'static, ()> {
        let either_frame = sv2_frame.into();
        self.sender.send(either_frame).await?;
        Ok(())
    }
}
