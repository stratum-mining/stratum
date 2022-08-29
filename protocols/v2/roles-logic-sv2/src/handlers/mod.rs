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
pub mod mining;
pub mod template_distribution;
use crate::utils::Mutex;
use std::sync::Arc;

/// SubProtocol is the SV2 (sub)protocol that the implementer is implementing (e.g.: `mining`,
/// `common`, ect.)
/// Remote is whatever type the implementer uses to represent the remote connection.
pub enum SendTo_<SubProtocol, Remote> {
    /// Used by SV2-only proxies to allocate a new message + relay.
    RelayNewMessageToSv2(Arc<Mutex<Remote>>, SubProtocol),
    /// Used by SV2-only proxies to relay the same message it receives.
    /// one.
    RelaySameMessageToSv2(Arc<Mutex<Remote>>),
    /// Used by SV1<->SV2 translator proxies to relay a SV2 message to be translated to a SV1
    /// message by the proxy.
    RelaySameMessageToSv1(SubProtocol),
    /// Used by all proxies and other roles to directly respond to a received SV2 message with
    /// the proper SV2 message response.
    Respond(SubProtocol),
    /// Return multiple types of `SendTo`, e.g. `SendTo::Respond` + `SendTo::Relay*`. In the case
    /// where multiple `SendTo::Relay*` is received in this variant, that indicates there are
    /// multiple Downstream roles connected to the application.
    Multiple(Vec<SendTo_<SubProtocol, Remote>>),
    /// Used by all proxies and other roles when no messages need to be sent.
    None(Option<SubProtocol>),
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::RelayNewMessageToSv2(_, m) => Some(m),
            Self::RelaySameMessageToSv2(_) => None,
            Self::RelaySameMessageToSv1(m) => Some(m),
            Self::Respond(m) => Some(m),
            Self::Multiple(_) => None,
            Self::None(m) => m,
        }
    }
    pub fn into_remote(self) -> Option<Arc<Mutex<Remote>>> {
        match self {
            Self::RelayNewMessageToSv2(r, _) => Some(r),
            Self::RelaySameMessageToSv2(r) => Some(r),
            Self::RelaySameMessageToSv1(_) => None,
            Self::Respond(_) => None,
            Self::Multiple(_) => None,
            Self::None(_) => None,
        }
    }
}
