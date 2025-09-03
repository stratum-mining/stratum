use crate::{
    interceptor::{InterceptAction, MessageDirection},
    message_aggregator::MessagesAggregator,
    types::MsgType,
    utils::{
        create_downstream, create_upstream, recv_from_down_send_to_up, recv_from_up_send_to_down,
        wait_for_client,
    },
};
use std::net::SocketAddr;
use stratum_common::roles_logic_sv2::parsers_sv2::{message_type_to_name, AnyMessage};
use tokio::{net::TcpStream, select};

const DEFAULT_TIMEOUT: u64 = 60;

/// Allows to intercept messages sent between two roles.
///
/// Can be useful for testing purposes, as it allows to assert that the roles have sent specific
/// messages in a specific order and to inspect the messages details.
///
/// The downstream (or client) role connects to the [`Sniffer`] `listening_address` and the
/// [`Sniffer`] connects to the `upstream` server. This way, the Sniffer can intercept messages sent
/// between the downstream and upstream roles.
///
/// Messages received from downstream are stored in the `messages_from_downstream` aggregator and
/// forwarded to the upstream role. Alternatively, messages received from upstream are stored in
/// the `messages_from_upstream` and forwarded to the downstream role. Both
/// `messages_from_downstream` and `messages_from_upstream` aggregators can be accessed as FIFO
/// queues via [`Sniffer::next_message_from_downstream`] and
/// [`Sniffer::next_message_from_upstream`], respectively.
///
/// The `timeout` parameter can be used to configure the timeout for the sniffer. If not provided,
/// the default timeout is 1 minute.
///
/// In order to replace or ignore the messages sent between the roles, [`InterceptAction`] can be
/// used in [`Sniffer::new`].
#[derive(Debug, Clone)]
pub struct Sniffer<'a> {
    identifier: &'a str,
    listening_address: SocketAddr,
    upstream_address: SocketAddr,
    messages_from_downstream: MessagesAggregator,
    messages_from_upstream: MessagesAggregator,
    check_on_drop: bool,
    action: Vec<InterceptAction>,
    timeout: Option<u64>,
}

impl<'a> Sniffer<'a> {
    /// Creates a new sniffer that listens on the given listening address and connects to the given
    /// upstream address.
    pub fn new(
        identifier: &'a str,
        listening_address: SocketAddr,
        upstream_address: SocketAddr,
        check_on_drop: bool,
        action: Vec<InterceptAction>,
        timeout: Option<u64>,
    ) -> Self {
        Self {
            identifier,
            listening_address,
            upstream_address,
            messages_from_downstream: MessagesAggregator::new(),
            messages_from_upstream: MessagesAggregator::new(),
            check_on_drop,
            action,
            timeout,
        }
    }

    /// Starts the sniffer.
    ///
    /// The sniffer should be started after the upstream role have been initialized and is ready to
    /// accept messages and before the downstream role starts sending messages.
    pub fn start(&self) {
        let listening_address = self.listening_address;
        let upstream_address = self.upstream_address;
        let messages_from_downstream = self.messages_from_downstream.clone();
        let messages_from_upstream = self.messages_from_upstream.clone();
        let action = self.action.clone();
        let identifier = self.identifier.to_string();
        tokio::spawn(async move {
            let (downstream_receiver, downstream_sender) =
                create_downstream(wait_for_client(listening_address).await)
                    .await
                    .expect("Failed to create downstream");
            let (upstream_receiver, upstream_sender) = create_upstream(
                TcpStream::connect(upstream_address)
                    .await
                    .expect("Failed to connect to upstream"),
            )
            .await
            .expect("Failed to create upstream");
            select! {
                _ = tokio::signal::ctrl_c() => { },
                _ = recv_from_down_send_to_up(downstream_receiver, upstream_sender, messages_from_downstream, action.clone(), &identifier) => { },
                _ = recv_from_up_send_to_down(upstream_receiver, downstream_sender, messages_from_upstream, action, &identifier) => { },
            };
        });
    }

    /// Returns the oldest message sent by downstream.
    ///
    /// The queue is FIFO and once a message is returned it is removed from the queue.
    ///
    /// This can be used to assert that the downstream sent:
    /// - specific message types
    /// - specific message fields
    pub fn next_message_from_downstream(&self) -> Option<(MsgType, AnyMessage<'static>)> {
        self.messages_from_downstream.next_message()
    }

    /// Returns the oldest message sent by upstream.
    ///
    /// The queue is FIFO and once a message is returned it is removed from the queue.
    ///
    /// This can be used to assert that the upstream sent:
    /// - specific message types
    /// - specific message fields
    pub fn next_message_from_upstream(&self) -> Option<(MsgType, AnyMessage<'static>)> {
        self.messages_from_upstream.next_message()
    }

    /// Waits until a message of the specified type is received into the `message_direction`
    /// corresponding queue.
    pub async fn wait_for_message_type(
        &self,
        message_direction: MessageDirection,
        message_type: u8,
    ) {
        let now = std::time::Instant::now();
        loop {
            let has_message_type = match message_direction {
                MessageDirection::ToDownstream => {
                    self.messages_from_upstream.has_message_type(message_type)
                }
                MessageDirection::ToUpstream => {
                    self.messages_from_downstream.has_message_type(message_type)
                }
            };

            // ready to unblock test runtime
            if has_message_type {
                return;
            }

            // configurable timeout, 1 minute default
            if now.elapsed().as_secs() > self.timeout.unwrap_or(DEFAULT_TIMEOUT) {
                panic!(
                    "timeout while waiting for message {} to go {}",
                    message_type_to_name(message_type),
                    message_direction
                );
            }

            // sleep to reduce async lock contention
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    /// Assert message is not present in the queue
    ///
    /// Will return true if the message is not present in the queue, false otherwise.
    pub async fn assert_message_not_present(
        &self,
        message_direction: MessageDirection,
        message_type: u8,
    ) -> bool {
        let has_message_type = match message_direction {
            MessageDirection::ToDownstream => {
                self.messages_from_upstream.has_message_type(message_type)
            }
            MessageDirection::ToUpstream => {
                self.messages_from_downstream.has_message_type(message_type)
            }
        };
        !has_message_type
    }

    /// Similar to `[Sniffer::wait_for_message_type]` but also removes the messages from the queue
    /// including the specified message type.
    pub async fn wait_for_message_type_and_clean_queue(
        &self,
        message_direction: MessageDirection,
        message_type: u8,
    ) -> bool {
        let now = std::time::Instant::now();
        loop {
            let has_message_type = match message_direction {
                MessageDirection::ToDownstream => self
                    .messages_from_upstream
                    .has_message_type_with_remove(message_type),
                MessageDirection::ToUpstream => self
                    .messages_from_downstream
                    .has_message_type_with_remove(message_type),
            };

            // ready to unblock test runtime
            if has_message_type {
                return true;
            }

            // configurable timeout, 1 minute default
            if now.elapsed().as_secs() > self.timeout.unwrap_or(DEFAULT_TIMEOUT) {
                panic!(
                    "timeout while waiting for message {} to go {}",
                    message_type_to_name(message_type),
                    message_direction
                );
            }

            // sleep to reduce async lock contention
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    /// Checks whether the sniffer has received a message of the specified type.
    pub fn includes_message_type(
        &self,
        message_direction: MessageDirection,
        message_type: u8,
    ) -> bool {
        match message_direction {
            MessageDirection::ToDownstream => {
                self.messages_from_upstream.has_message_type(message_type)
            }
            MessageDirection::ToUpstream => {
                self.messages_from_downstream.has_message_type(message_type)
            }
        }
    }
}

// Utility macro to assert that the downstream and upstream roles have sent specific messages.
//
// This macro can be called in two ways:
// 1. If you want to assert the message without any of its properties, you can invoke the macro
//   with the message group, the nested message group, the message, and the expected message:
//   `assert_message!(TemplateDistribution, TemplateDistribution, $msg,
// $expected_message_variant);`.
//
// 2. If you want to assert the message with its properties, you can invoke the macro with the
//  message group, the nested message group, the message, the expected message, and the expected
//  properties and values:
//  `assert_message!(TemplateDistribution, TemplateDistribution, $msg, $expected_message_variant,
//  $expected_property, $expected_property_value, ...);`.
//  Note that you can provide any number of properties and values.
//
//  In both cases, the `$message_group` could be any variant of `AnyMessage::$message_group` and
//  the `$nested_message_group` could be any variant of
//  `AnyMessage::$message_group($nested_message_group)`.
//
//  If you dont want to provide the `$message_group` and `$nested_message_group` arguments, you can
//  utilize `assert_common_message!`, `assert_tp_message!`, `assert_mining_message!`, and
//  `assert_jd_message!` macros. All those macros are just wrappers around `assert_message!` macro
//  with predefined `$message_group` and `$nested_message_group` arguments. They also can be called
//  in two ways, with or without properties validation.
#[macro_export]
macro_rules! assert_message {
  ($message_group:ident, $nested_message_group:ident, $msg:expr, $expected_message_variant:ident,
   $($expected_property:ident, $expected_property_value:expr),*) => { match $msg {
	  Some((_, message)) => {
		match message {
		  AnyMessage::$message_group($nested_message_group::$expected_message_variant(
			  $expected_message_variant {
				$($expected_property,)*
				  ..
			  },
		  )) => {
			$(
			  assert_eq!($expected_property.clone(), $expected_property_value);
			)*
		  }
		  _ => {
			panic!(
			  "Sent wrong message: {:?}",
			  message
			);
		  }
		}
	  }
	  _ => panic!("No message received"),
		}
  };
  ($message_group:ident, $nested_message_group:ident, $msg:expr, $expected_message_variant:ident) => {
	match $msg {
	  Some((_, message)) => {
		match message {
		  AnyMessage::$message_group($nested_message_group::$expected_message_variant(_)) => {}
		  _ => {
			panic!(
			  "Sent wrong message: {:?}",
			  message
			);
		  }
		}
	  }
	  _ => panic!("No message received"),
		}
  };
}

// Assert that the message is a common message and that it has the expected properties and values.
#[macro_export]
macro_rules! assert_common_message {
  ($msg:expr, $expected_message_variant:ident, $($expected_property:ident, $expected_property_value:expr),*) => {
	assert_message!(Common, CommonMessages, $msg, $expected_message_variant, $($expected_property, $expected_property_value),*);
  };
  ($msg:expr, $expected_message_variant:ident) => {
	assert_message!(Common, CommonMessages, $msg, $expected_message_variant);
  };
}

// Assert that the message is a template distribution message and that it has the expected
// properties and values.
#[macro_export]
macro_rules! assert_tp_message {
  ($msg:expr, $expected_message_variant:ident, $($expected_property:ident, $expected_property_value:expr),*) => {
	assert_message!(TemplateDistribution, TemplateDistribution, $msg, $expected_message_variant, $($expected_property, $expected_property_value),*);
  };
  ($msg:expr, $expected_message_variant:ident) => {
	assert_message!(TemplateDistribution, TemplateDistribution, $msg, $expected_message_variant);
  };
}

// Assert that the message is a mining message and that it has the expected properties and values.
#[macro_export]
macro_rules! assert_mining_message {
  ($msg:expr, $expected_message_variant:ident, $($expected_property:ident, $expected_property_value:expr),*) => {
	assert_message!(Mining, Mining, $msg, $expected_message_variant, $($expected_property, $expected_property_value),*);
  };
  ($msg:expr, $expected_message_variant:ident) => {
	assert_message!(Mining, Mining, $msg, $expected_message_variant);
  };
}

// Assert that the message is a job declaration message and that it has the expected properties and
// values.
#[macro_export]
macro_rules! assert_jd_message {
  ($msg:expr, $expected_message_variant:ident, $($expected_property:ident, $expected_property_value:expr),*) => {
	assert_message!(JobDeclaration, JobDeclaration, $msg, $expected_message_variant, $($expected_property, $expected_property_value),*);
  };
  ($msg:expr, $expected_message_variant:ident) => {
	assert_message!(JobDeclaration, JobDeclaration, $msg, $expected_message_variant);
  };
}

// This implementation is used in order to check if a test has handled all messages sent by the
// downstream and upstream roles. If not, the test will panic.
//
// This is useful to ensure that the test has checked all exchanged messages between the roles.
impl Drop for Sniffer<'_> {
    fn drop(&mut self) {
        if self.check_on_drop {
            match (
                self.messages_from_downstream.is_empty(),
                self.messages_from_upstream.is_empty(),
            ) {
                (true, true) => {}
                (true, false) => {
                    println!(
                        "Sniffer {}: You didn't handle all upstream messages: {:?}",
                        self.identifier, self.messages_from_upstream
                    );
                    panic!();
                }
                (false, true) => {
                    println!(
                        "Sniffer {}: You didn't handle all downstream messages: {:?}",
                        self.identifier, self.messages_from_downstream
                    );
                    panic!();
                }
                (false, false) => {
                    println!(
                        "Sniffer {}: You didn't handle all downstream messages: {:?}",
                        self.identifier, self.messages_from_downstream
                    );
                    println!(
                        "Sniffer {}: You didn't handle all upstream messages: {:?}",
                        self.identifier, self.messages_from_upstream
                    );
                    panic!();
                }
            }
        }
    }
}
