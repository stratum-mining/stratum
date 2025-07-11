use std::{collections::VecDeque, sync::Arc};
use stratum_common::roles_logic_sv2::{parsers_sv2::AnyMessage, utils::Mutex};

use crate::types::MsgType;

#[derive(Debug, Clone)]
pub struct MessagesAggregator {
    messages: Arc<Mutex<VecDeque<(MsgType, AnyMessage<'static>)>>>,
}

impl Default for MessagesAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl MessagesAggregator {
    /// Creates a new [`MessagesAggregator`].
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Adds a message to the end of the queue.
    pub fn add_message(&self, msg_type: MsgType, message: AnyMessage<'static>) {
        self.messages
            .safe_lock(|messages| messages.push_back((msg_type, message)))
            .unwrap();
    }

    /// Returns false if the queue is empty, true otherwise.
    pub fn is_empty(&self) -> bool {
        self.messages
            .safe_lock(|messages| messages.is_empty())
            .unwrap()
    }

    /// returns true if contains message_type
    pub fn has_message_type(&self, message_type: u8) -> bool {
        let has_message: bool = self
            .messages
            .safe_lock(|messages| {
                for (t, _) in messages.iter() {
                    if *t == message_type {
                        return true; // Exit early with `true`
                    }
                }
                false // Default value if no match is found
            })
            .unwrap();
        has_message
    }

    /// returns true if contains message_type and removes messages from the queue
    /// until the first message of type message_type.
    pub fn has_message_type_with_remove(&self, message_type: u8) -> bool {
        self.messages
            .safe_lock(|messages| {
                let mut cloned_messages = messages.clone();
                for (pos, (t, _)) in cloned_messages.iter().enumerate() {
                    if *t == message_type {
                        let drained = cloned_messages.drain(pos + 1..).collect();
                        *messages = drained;
                        return true;
                    }
                }
                false
            })
            .unwrap()
    }

    /// The aggregator queues messages in FIFO order, so this function returns the oldest message in
    /// the queue.
    ///
    /// The returned message is removed from the queue.
    pub fn next_message(&self) -> Option<(MsgType, AnyMessage<'static>)> {
        let is_state = self
            .messages
            .safe_lock(|messages| {
                let mut cloned = messages.clone();
                if let Some((msg_type, msg)) = cloned.pop_front() {
                    *messages = cloned;
                    Some((msg_type, msg))
                } else {
                    None
                }
            })
            .unwrap();
        is_state
    }
}
