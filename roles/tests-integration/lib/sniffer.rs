use async_channel::{Receiver, Sender};
use codec_sv2::{
    framing_sv2::framing::Frame, HandshakeRole, Initiator, Responder, StandardEitherFrame, Sv2Frame,
};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection::Connection;
use roles_logic_sv2::{
    parsers::{
        AnyMessage, CommonMessages, IsSv2Message,
        JobDeclaration::{
            AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
            DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactions,
            IdentifyTransactionsSuccess, ProvideMissingTransactions,
            ProvideMissingTransactionsSuccess, SubmitSolution,
        },
        TemplateDistribution::{self, CoinbaseOutputDataSize},
    },
    utils::Mutex,
};
use std::{collections::VecDeque, convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    time::{sleep, Duration},
};
type MessageFrame = StandardEitherFrame<AnyMessage<'static>>;
type MsgType = u8;

#[derive(Debug, PartialEq)]
enum SnifferError {
    DownstreamClosed,
    UpstreamClosed,
}

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
/// In order to replace the messages sent between the roles, a set of [`InterceptMessage`] can be
/// used in [`Sniffer::new`].
#[derive(Debug, Clone)]
pub struct Sniffer {
    identifier: String,
    listening_address: SocketAddr,
    upstream_address: SocketAddr,
    messages_from_downstream: MessagesAggregator,
    messages_from_upstream: MessagesAggregator,
    check_on_drop: bool,
    action: Option<Action>,
}

/// Represents an action that [`Sniffer`] can take on intercepted messages.
#[derive(Debug, Clone)]
pub enum Action {
    /// Blocks the message stream after encountering a specific message.
    IgnoreFromMessage(IgnoreFromMessage),
    /// Intercepts and modifies a message before forwarding it.
    InterceptMessage(Box<InterceptMessage>),
}

impl Action {
    /// Returns the action if it is a downstream `IgnoreFromMessage` or `InterceptMessage`  
    /// with the specified message type.
    pub fn filter_downstream(
        action: &Option<Action>,
        msg_type: MsgType,
        direction: MessageDirection,
    ) -> Option<&Self> {
        action.as_ref().and_then(|action| match action {
            Action::IgnoreFromMessage(bm)
                if bm.direction == direction && bm.expected_message_type == msg_type =>
            {
                Some(action)
            }

            Action::InterceptMessage(im)
                if im.direction == direction && im.expected_message_type == msg_type =>
            {
                Some(action)
            }

            _ => None,
        })
    }
}
/// Defines an action that blocks the message stream after detecting a specific message.
#[derive(Debug, Clone)]
pub struct IgnoreFromMessage {
    direction: MessageDirection,
    expected_message_type: MsgType,
}

impl IgnoreFromMessage {
    /// Creates a new [`IgnoreFromMessage`] action.
    ///
    /// - `direction`: The direction of the message stream to block.
    /// - `expected_message_type`: The type of message after which the stream should be blocked.
    pub fn new(direction: MessageDirection, expected_message_type: MsgType) -> Self {
        IgnoreFromMessage {
            direction,
            expected_message_type,
        }
    }
}

impl From<IgnoreFromMessage> for Action {
    fn from(value: IgnoreFromMessage) -> Self {
        Action::IgnoreFromMessage(value)
    }
}

/// Allows [`Sniffer`] to replace some intercepted message before forwarding it.
#[derive(Debug, Clone)]
pub struct InterceptMessage {
    direction: MessageDirection,
    expected_message_type: MsgType,
    replacement_message: AnyMessage<'static>,
}

impl InterceptMessage {
    /// Constructor of `InterceptMessage`
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

impl From<InterceptMessage> for Action {
    fn from(value: InterceptMessage) -> Self {
        Action::InterceptMessage(Box::new(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageDirection {
    ToDownstream,
    ToUpstream,
}

impl Sniffer {
    /// Creates a new sniffer that listens on the given listening address and connects to the given
    /// upstream address.
    pub async fn new(
        identifier: String,
        listening_address: SocketAddr,
        upstream_address: SocketAddr,
        check_on_drop: bool,
        action: Option<Action>,
    ) -> Self {
        Self {
            identifier,
            listening_address,
            upstream_address,
            messages_from_downstream: MessagesAggregator::new(),
            messages_from_upstream: MessagesAggregator::new(),
            check_on_drop,
            action,
        }
    }

    /// Starts the sniffer.
    ///
    /// The sniffer should be started after the upstream role have been initialized and is ready to
    /// accept messages and before the downstream role starts sending messages.
    pub async fn start(self) {
        let (downstream_receiver, downstream_sender) =
            Self::create_downstream(Self::wait_for_client(self.listening_address).await)
                .await
                .expect("Failed to create downstream");
        let (upstream_receiver, upstream_sender) = Self::create_upstream(
            TcpStream::connect(self.upstream_address)
                .await
                .expect("Failed to connect to upstream"),
        )
        .await
        .expect("Failed to create upstream");
        let downstream_messages = self.messages_from_downstream.clone();
        let upstream_messages = self.messages_from_upstream.clone();
        let action = self.action.clone();
        let _ = select! {
            r = Self::recv_from_down_send_to_up(downstream_receiver, upstream_sender, downstream_messages, action.clone()) => r,
            r = Self::recv_from_up_send_to_down(upstream_receiver, downstream_sender, upstream_messages, action) => r,
        };
        // wait a bit so we dont drop the sniffer before the test has finished
        sleep(std::time::Duration::from_secs(1)).await;
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

    async fn create_downstream(
        stream: TcpStream,
    ) -> Option<(Receiver<MessageFrame>, Sender<MessageFrame>)> {
        let pub_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
            .to_string()
            .parse::<Secp256k1PublicKey>()
            .unwrap()
            .into_bytes();
        let prv_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
            .to_string()
            .parse::<Secp256k1SecretKey>()
            .unwrap()
            .into_bytes();
        let responder =
            Responder::from_authority_kp(&pub_key, &prv_key, std::time::Duration::from_secs(10000))
                .unwrap();
        if let Ok((receiver_from_client, sender_to_client, _, _)) =
            Connection::new::<'static, AnyMessage<'static>>(
                stream,
                HandshakeRole::Responder(responder),
            )
            .await
        {
            Some((receiver_from_client, sender_to_client))
        } else {
            None
        }
    }

    async fn create_upstream(
        stream: TcpStream,
    ) -> Option<(Receiver<MessageFrame>, Sender<MessageFrame>)> {
        let initiator = Initiator::without_pk().expect("This fn call can not fail");
        if let Ok((receiver_from_server, sender_to_server, _, _)) =
            Connection::new::<'static, AnyMessage<'static>>(
                stream,
                HandshakeRole::Initiator(initiator),
            )
            .await
        {
            Some((receiver_from_server, sender_to_server))
        } else {
            None
        }
    }

    async fn recv_from_down_send_to_up(
        recv: Receiver<MessageFrame>,
        send: Sender<MessageFrame>,
        downstream_messages: MessagesAggregator,
        action: Option<Action>,
    ) -> Result<(), SnifferError> {
        let mut blocked = false;
        while let Ok(mut frame) = recv.recv().await {
            if blocked {
                continue;
            }
            let (msg_type, msg) = Self::message_from_frame(&mut frame);
            let action = action.as_ref().and_then(|action| match action {
                Action::BlockFromMessage(bm)
                    if bm.direction == MessageDirection::ToUpstream
                        && bm.expected_message_type == msg_type =>
                {
                    Some(action)
                }

                Action::InterceptMessage(im)
                    if im.direction == MessageDirection::ToUpstream
                        && im.expected_message_type == msg_type =>
                {
                    Some(action)
                }

                _ => None,
            });
            if let Some(ref action) = action {
                match action {
                    Action::IgnoreFromMessage(_) => {
                        blocked = true;
                        continue;
                    }
                    Action::InterceptMessage(intercept_message) => {
                        let intercept_frame = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
                            Sv2Frame::from_message(
                                intercept_message.replacement_message.clone(),
                                intercept_message.replacement_message.message_type(),
                                0,
                                false,
                            )
                            .expect("Failed to create the frame"),
                        );
                        downstream_messages.add_message(
                            intercept_message.replacement_message.message_type(),
                            intercept_message.replacement_message.clone(),
                        );
                        send.send(intercept_frame)
                            .await
                            .map_err(|_| SnifferError::UpstreamClosed)?;
                    }
                }
            } else {
                downstream_messages.add_message(msg_type, msg);
                send.send(frame)
                    .await
                    .map_err(|_| SnifferError::UpstreamClosed)?;
            }
        }
        Err(SnifferError::DownstreamClosed)
    }

    async fn recv_from_up_send_to_down(
        recv: Receiver<MessageFrame>,
        send: Sender<MessageFrame>,
        upstream_messages: MessagesAggregator,
        action: Option<Action>,
    ) -> Result<(), SnifferError> {
        let mut blocked = false;
        while let Ok(mut frame) = recv.recv().await {
            if blocked {
                continue;
            }
            let (msg_type, msg) = Self::message_from_frame(&mut frame);

            let action = action.as_ref().and_then(|action| match action {
                Action::BlockFromMessage(bm)
                    if bm.direction == MessageDirection::ToDownstream
                        && bm.expected_message_type == msg_type =>
                {
                    Some(action)
                }

                Action::InterceptMessage(im)
                    if im.direction == MessageDirection::ToDownstream
                        && im.expected_message_type == msg_type =>
                {
                    Some(action)
                }

                _ => None,
            });

            if let Some(ref action) = action {
                match action {
                    Action::IgnoreFromMessage(_) => {
                        blocked = true;
                        continue;
                    }
                    Action::InterceptMessage(intercept_message) => {
                        let intercept_frame = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
                            Sv2Frame::from_message(
                                intercept_message.replacement_message.clone(),
                                intercept_message.replacement_message.message_type(),
                                0,
                                false,
                            )
                            .expect("Failed to create the frame"),
                        );
                        upstream_messages.add_message(
                            intercept_message.replacement_message.message_type(),
                            intercept_message.replacement_message.clone(),
                        );
                        send.send(intercept_frame)
                            .await
                            .map_err(|_| SnifferError::DownstreamClosed)?;
                    }
                }
            } else {
                upstream_messages.add_message(msg_type, msg);
                send.send(frame)
                    .await
                    .map_err(|_| SnifferError::DownstreamClosed)?;
            }
        }
        Err(SnifferError::UpstreamClosed)
    }

    fn message_from_frame(frame: &mut MessageFrame) -> (MsgType, AnyMessage<'static>) {
        match frame {
            Frame::Sv2(frame) => {
                if let Some(header) = frame.get_header() {
                    let message_type = header.msg_type();
                    let mut payload = frame.payload().to_vec();
                    let message: Result<AnyMessage<'_>, _> =
                        (message_type, payload.as_mut_slice()).try_into();
                    match message {
                        Ok(message) => {
                            let message = Self::into_static(message);
                            (message_type, message)
                        }
                        _ => {
                            println!(
                                "Received frame with invalid payload or message type: {frame:?}"
                            );
                            panic!();
                        }
                    }
                } else {
                    println!("Received frame with invalid header: {frame:?}");
                    panic!();
                }
            }
            Frame::HandShake(f) => {
                println!("Received unexpected handshake frame: {f:?}");
                panic!();
            }
        }
    }

    fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
        match m {
            AnyMessage::Mining(m) => AnyMessage::Mining(m.into_static()),
            AnyMessage::Common(m) => match m {
                CommonMessages::ChannelEndpointChanged(m) => {
                    AnyMessage::Common(CommonMessages::ChannelEndpointChanged(m.into_static()))
                }
                CommonMessages::SetupConnection(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnection(m.into_static()))
                }
                CommonMessages::SetupConnectionError(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnectionError(m.into_static()))
                }
                CommonMessages::SetupConnectionSuccess(m) => {
                    AnyMessage::Common(CommonMessages::SetupConnectionSuccess(m.into_static()))
                }
            },
            AnyMessage::JobDeclaration(m) => match m {
                AllocateMiningJobToken(m) => {
                    AnyMessage::JobDeclaration(AllocateMiningJobToken(m.into_static()))
                }
                AllocateMiningJobTokenSuccess(m) => {
                    AnyMessage::JobDeclaration(AllocateMiningJobTokenSuccess(m.into_static()))
                }
                DeclareMiningJob(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJob(m.into_static()))
                }
                DeclareMiningJobError(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJobError(m.into_static()))
                }
                DeclareMiningJobSuccess(m) => {
                    AnyMessage::JobDeclaration(DeclareMiningJobSuccess(m.into_static()))
                }
                IdentifyTransactions(m) => {
                    AnyMessage::JobDeclaration(IdentifyTransactions(m.into_static()))
                }
                IdentifyTransactionsSuccess(m) => {
                    AnyMessage::JobDeclaration(IdentifyTransactionsSuccess(m.into_static()))
                }
                ProvideMissingTransactions(m) => {
                    AnyMessage::JobDeclaration(ProvideMissingTransactions(m.into_static()))
                }
                ProvideMissingTransactionsSuccess(m) => {
                    AnyMessage::JobDeclaration(ProvideMissingTransactionsSuccess(m.into_static()))
                }
                SubmitSolution(m) => AnyMessage::JobDeclaration(SubmitSolution(m.into_static())),
            },
            AnyMessage::TemplateDistribution(m) => match m {
                CoinbaseOutputDataSize(m) => {
                    AnyMessage::TemplateDistribution(CoinbaseOutputDataSize(m.into_static()))
                }
                TemplateDistribution::NewTemplate(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::NewTemplate(m.into_static()),
                ),
                TemplateDistribution::RequestTransactionData(m) => {
                    AnyMessage::TemplateDistribution(TemplateDistribution::RequestTransactionData(
                        m.into_static(),
                    ))
                }
                TemplateDistribution::RequestTransactionDataError(m) => {
                    AnyMessage::TemplateDistribution(
                        TemplateDistribution::RequestTransactionDataError(m.into_static()),
                    )
                }
                TemplateDistribution::RequestTransactionDataSuccess(m) => {
                    AnyMessage::TemplateDistribution(
                        TemplateDistribution::RequestTransactionDataSuccess(m.into_static()),
                    )
                }
                TemplateDistribution::SetNewPrevHash(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::SetNewPrevHash(m.into_static()),
                ),
                TemplateDistribution::SubmitSolution(m) => AnyMessage::TemplateDistribution(
                    TemplateDistribution::SubmitSolution(m.into_static()),
                ),
            },
        }
    }

    async fn wait_for_client(listen_socket: SocketAddr) -> TcpStream {
        let listener = TcpListener::bind(listen_socket)
            .await
            .expect("Impossible to listen on given address");
        if let Ok((stream, _)) = listener.accept().await {
            stream
        } else {
            panic!("Impossible to accept dowsntream connection")
        }
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

            // 10 min timeout
            // only for worst case, ideally should never be triggered
            if now.elapsed().as_secs() > 10 * 60 {
                panic!("Timeout waiting for message type");
            }

            // sleep to reduce async lock contention
            sleep(Duration::from_secs(1)).await;
        }
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

            // 10 min timeout
            // only for worst case, ideally should never be triggered
            if now.elapsed().as_secs() > 10 * 60 {
                panic!("Timeout waiting for message type");
            }

            // sleep to reduce async lock contention
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Checks whether the sniffer has received a message of the specified type.
    pub async fn includes_message_type(
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
impl Drop for Sniffer {
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

#[derive(Debug, Clone)]
struct MessagesAggregator {
    messages: Arc<Mutex<VecDeque<(MsgType, AnyMessage<'static>)>>>,
}

impl MessagesAggregator {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    // Adds a message to the end of the queue.
    fn add_message(&self, msg_type: MsgType, message: AnyMessage<'static>) {
        self.messages
            .safe_lock(|messages| messages.push_back((msg_type, message)))
            .unwrap();
    }

    fn is_empty(&self) -> bool {
        self.messages
            .safe_lock(|messages| messages.is_empty())
            .unwrap()
    }

    // returns true if contains message_type
    fn has_message_type(&self, message_type: u8) -> bool {
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

    fn has_message_type_with_remove(&self, message_type: u8) -> bool {
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

    // The aggregator queues messages in FIFO order, so this function returns the oldest message in
    // the queue.
    //
    // The returned message is removed from the queue.
    fn next_message(&self) -> Option<(MsgType, AnyMessage<'static>)> {
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
