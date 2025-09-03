use std::fmt;

use crate::types::MsgType;
use stratum_common::roles_logic_sv2::parsers_sv2::AnyMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageDirection {
    ToDownstream,
    ToUpstream,
}

impl fmt::Display for MessageDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageDirection::ToDownstream => write!(f, "downstream"),
            MessageDirection::ToUpstream => write!(f, "upstream"),
        }
    }
}

/// Represents an action that [`Sniffer`] can take on intercepted messages.
#[derive(Debug, Clone)]
pub enum InterceptAction {
    /// Prevents a message from being forwarded and stored into the message aggregator.
    IgnoreMessage(IgnoreMessage),
    /// Intercepts and modifies a message before forwarding it.
    ReplaceMessage(Box<ReplaceMessage>),
}

impl InterceptAction {
    /// Returns the action if it is `IgnoreMessage` or `ReplaceMessage`
    /// with the specified message type.
    pub fn find_matching_action(
        &self,
        msg_type: MsgType,
        direction: MessageDirection,
    ) -> Option<&Self> {
        match self {
            InterceptAction::IgnoreMessage(bm)
                if bm.direction == direction && bm.expected_message_type == msg_type =>
            {
                Some(self)
            }

            InterceptAction::ReplaceMessage(im)
                if im.direction == direction && im.expected_message_type == msg_type =>
            {
                Some(self)
            }

            _ => None,
        }
    }
}
/// Defines an action that prevents a message from being forwarded.
///
/// When a message matching the specified type and direction is intercepted,
/// it will not be added to the message aggregator for inspection and will not be
/// forwarded to the destination. All other messages will continue to be forwarded normally.
#[derive(Debug, Clone)]
pub struct IgnoreMessage {
    direction: MessageDirection,
    expected_message_type: MsgType,
}

impl IgnoreMessage {
    /// Creates a new [`IgnoreMessage`] action.
    ///
    /// - `direction`: The direction of the message to be ignored.
    /// - `expected_message_type`: The type of message to be ignored.
    pub fn new(direction: MessageDirection, expected_message_type: MsgType) -> Self {
        IgnoreMessage {
            direction,
            expected_message_type,
        }
    }
}

impl From<IgnoreMessage> for InterceptAction {
    fn from(value: IgnoreMessage) -> Self {
        InterceptAction::IgnoreMessage(value)
    }
}

/// Allows [`Sniffer`] to replace some intercepted message before forwarding it.
#[derive(Debug, Clone)]
pub struct ReplaceMessage {
    direction: MessageDirection,
    expected_message_type: MsgType,
    pub(crate) replacement_message: AnyMessage<'static>,
}

impl ReplaceMessage {
    /// Constructor of `ReplaceMessage`
    /// - `direction`: direction of message to be intercepted and replaced
    /// - `expected_message_type`: type of message to be intercepted and replaced
    /// - `replacement_message`: message to replace the intercepted one
    /// - `replacement_message_type`: type of message to replace the intercepted one
    pub fn new(
        direction: MessageDirection,
        expected_message_type: MsgType,
        replacement_message: AnyMessage<'static>,
    ) -> Self {
        Self {
            direction,
            expected_message_type,
            replacement_message,
        }
    }
}

impl From<ReplaceMessage> for InterceptAction {
    fn from(value: ReplaceMessage) -> Self {
        InterceptAction::ReplaceMessage(Box::new(value))
    }
}
