//! Handlers are divided per (sub)protocol and per downstream/upstream
//! Each (sup)protocol define an handler for both the upstream node and the downstream node
//! Handlers are trait called Parse[Downstream/Upstream][(sub)protocol] (eg. ParseDownstreamCommonMessages)
//!
//! When implemented an handler make avaiable a funtction called
//! handle_message_[(sub)protoco](..) (eg handle_message_common(..))
//!
//! The trait require the implementor to define one function for each message type that a role
//! defined by the (sub)protocl and the upstream/downstream state could receive.
//!
//! This funtcion will always take a mutable ref to self, a message payload and a message type and
//! a routing logic.
//! Using parsers in crate::parser the payload and message type are parsed in an actual Sv2
//! message.
//! Routing logic is used in order to select the correct downstream/upstream to which the message
//! must be realyied/sent
//! Routing logic is used to update the request id when needed.
//! After that the specific function for the message type (implemented by the implementor) is
//! called with the Sv2 message and the remote that must receive the message.
//!
//! A Result<SendTo_, Error> is returned and is duty of the implementor to send the message
pub mod common;
pub mod mining;
pub mod template_distribution;
use crate::utils::Mutex;
use std::sync::Arc;

/// SubProtocol is the Sv2 (sub)protocol that the implementor is implementing (eg: mining, common,
/// ...)
/// Remote is wathever type the implementor use to represent remote connection
pub enum SendTo_<SubProtocol, Remote> {
    /// Used by proxyies to realy messages. It allocate a new message.
    RelayNewMessage(Arc<Mutex<Remote>>, SubProtocol),
    /// Used by proxyies to relay messages. It do not allocate a new message and use the received
    /// one.
    RelaySameMessage(Arc<Mutex<Remote>>),
    /// Used by proxyies and other roles to directly respond.
    Respond(SubProtocol),
    /// Used when multiple type of SendTo are needed
    Multiple(Vec<SendTo_<SubProtocol, Remote>>),
    /// Used by proxyies and other roles when no messages need to be sent.
    None,
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::RelayNewMessage(_, m) => Some(m),
            Self::RelaySameMessage(_) => None,
            Self::Respond(m) => Some(m),
            Self::Multiple(_) => None,
            Self::None => None,
        }
    }
    pub fn into_remote(self) -> Option<Arc<Mutex<Remote>>> {
        match self {
            Self::RelayNewMessage(r, _) => Some(r),
            Self::RelaySameMessage(r) => Some(r),
            Self::Respond(_) => None,
            Self::Multiple(_) => None,
            Self::None => None,
        }
    }
}
