//! Handlers are divided per (sub)protocol and per Downstream/Upstream.
//! Each (sup)protocol defines a handler for both the Upstream node and the Downstream node
//! Handlers are a trait called `Parse[Downstream/Upstream][(sub)protocol]`
//! (eg. `ParseDownstreamCommonMessages`).
//!
//! When implemented, the handler makes the `handle_message_[(sub)protoco](..)` (e.g.
//! `handle_message_common(..)`) function available.
//!
//! The trait requires the implementer to define one function for each message type that a role
//! defined by the (sub)protocol and the Upstream/Downstream state could receive.
//!
//! This function will always take a mutable ref to `self`, a message payload + message type, and
//! the routing logic.
//! Using `parsers` in `crate::parser`, the payload and message type are parsed in an actual SV2
//! message.
//! Routing logic is used in order to select the correct Downstream/Upstream to which the message
//! must be relayed/sent.
//! Routing logic is used to update the request id when needed.
//! After that, the specific function for the message type (implemented by the implementer) is
//! called with the SV2 message and the remote that must receive the message.
//!
//! A `Result<SendTo_, Error>` is returned and it is the duty of the implementer to send the
//! message.
//!
pub mod common;
pub mod job_declaration;
pub mod mining;
pub mod template_distribution;
use crate::utils::Mutex;
use std::sync::Arc;

#[derive(Debug)]
/// Message is a serializable entity that rapresent the meanings of communication between Remote(s)
/// SendTo_ is used to add context to Message, it say what we need to do with that Message.
pub enum SendTo_<Message, Remote> {
    /// Used by proxies when Message must be relayed downstream or upstream and we want to specify
    /// to which particular downstream or upstream we want to relay the message.
    ///
    /// When the message that we need to relay is the same message that we received should be used
    /// RelaySameMessageToRemote in order to save an allocation.
    RelayNewMessageToRemote(Arc<Mutex<Remote>>, Message),
    /// Used by proxies when Message must be relayed downstream or upstream and we want to specify
    /// to which particular downstream or upstream we want to relay the message.
    ///
    /// Is used when we need to relay the same message the we received in order to save an
    /// allocation.
    RelaySameMessageToRemote(Arc<Mutex<Remote>>),
    /// Used by proxies when Message must be relayed downstream or upstream and we do not want to specify
    /// specify to which particular downstream or upstream we want to relay the message.
    ///
    /// This is used in proxies that do and Sv1 to Sv2 translation. The upstream is connected via
    /// an extended channel that means that
    RelayNewMessage(Message),
    /// Used proxies clients and servers to directly respond to a received message.
    Respond(Message),
    Multiple(Vec<SendTo_<Message, Remote>>),
    /// Used by proxies, clients, and servers, when Message do not have to be used in any of the above way.
    /// If Message is still needed to be used in a non conventional way we use SendTo::None(Some(message))
    /// If we just want to discard it we can use SendTo::None(None)
    ///
    /// SendTo::None(Some(m)) could be used for example when we do not need to send the message,
    /// but we still need it for successive handling/transformation.
    /// One of these cases are proxies that are connected to upstream via an extended channel (like the
    /// Sv1 <-> Sv2 translator). This because extended channel messages are always general for all
    /// the downstream, where standard channel message can be specific for a particular downstream.
    /// Another case is when 2 roles are implemented in the same software, like a pool that is
    /// both TP client and a Mining server, messages received by the TP client must be sent to the
    /// Mining Server than transformed in Mining messages and sent to the downstream.
    ///
    None(Option<Message>),
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::RelayNewMessageToRemote(_, m) => Some(m),
            Self::RelaySameMessageToRemote(_) => None,
            Self::RelayNewMessage(m) => Some(m),
            Self::Respond(m) => Some(m),
            Self::Multiple(_) => None,
            Self::None(m) => m,
        }
    }
    pub fn into_remote(self) -> Option<Arc<Mutex<Remote>>> {
        match self {
            Self::RelayNewMessageToRemote(r, _) => Some(r),
            Self::RelaySameMessageToRemote(r) => Some(r),
            Self::RelayNewMessage(_) => None,
            Self::Respond(_) => None,
            Self::Multiple(_) => None,
            Self::None(_) => None,
        }
    }
}
