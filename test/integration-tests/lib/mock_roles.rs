use crate::{
    message_aggregator::MessagesAggregator,
    types::{MessageFrame, MsgType},
    utils::{create_downstream, create_upstream, message_from_frame, wait_for_client},
};
use async_channel::Sender;
use std::net::SocketAddr;
use stratum_common::roles_logic_sv2::{
    codec_sv2::{StandardEitherFrame, Sv2Frame},
    parsers_sv2::AnyMessage,
};
use tokio::net::TcpStream;

pub struct MockDownstream {
    upstream_address: SocketAddr,
    messages_from_upstream: MessagesAggregator,
}

impl MockDownstream {
    pub fn new(upstream_address: SocketAddr) -> Self {
        Self {
            upstream_address,
            messages_from_upstream: MessagesAggregator::new(),
        }
    }

    pub async fn start(&self) -> Sender<MessageFrame> {
        let upstream_address = self.upstream_address;
        let (upstream_receiver, upstream_sender) = create_upstream(loop {
            match TcpStream::connect(upstream_address).await {
                Ok(stream) => break stream,
                Err(_) => {
                    println!("MockDownstream: unable to connect to upstream, retrying");
                }
            }
        })
        .await
        .expect("Failed to create upstream");
        let messages_from_upstream = self.messages_from_upstream.clone();
        tokio::spawn(async move {
            while let Ok(mut frame) = upstream_receiver.recv().await {
                let (msg_type, msg) = message_from_frame(&mut frame);
                messages_from_upstream.add_message(msg_type, msg);
            }
        });
        upstream_sender
    }

    pub fn next_message_from_upstream(&self) -> Option<(MsgType, AnyMessage<'static>)> {
        self.messages_from_upstream.next_message()
    }
}

pub struct MockUpstream {
    listening_address: SocketAddr,
    messages_from_dowsntream: MessagesAggregator,
    // First item in tuple refer to the message(MsgType) received and second to what
    // response(AnyMessage) should the upstream send back.
    response_messages: Vec<(MsgType, AnyMessage<'static>)>,
}

impl MockUpstream {
    pub fn new(
        listening_address: SocketAddr,
        response_messages: Vec<(MsgType, AnyMessage<'static>)>,
    ) -> Self {
        Self {
            listening_address,
            messages_from_dowsntream: MessagesAggregator::new(),
            response_messages,
        }
    }

    pub async fn start(&self) {
        let listening_address = self.listening_address;
        let messages_from_dowsntream = self.messages_from_dowsntream.clone();
        let response_messages = self.response_messages.clone();
        tokio::spawn(async move {
            let (downstream_receiver, downstream_sender) =
                create_downstream(wait_for_client(listening_address).await)
                    .await
                    .expect("Failed to connect to downstream");
            while let Ok(mut frame) = downstream_receiver.recv().await {
                let (msg_type, msg) = message_from_frame(&mut frame);
                // save messages received from downstream
                messages_from_dowsntream.add_message(msg_type, msg);
                // find a response if the user provided one
                let response = response_messages
                    .iter()
                    .find(|(m_type, _)| m_type == &msg_type);
                // send response back to the downstream if found
                if let Some((_, response_msg)) = response {
                    let message = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
                        Sv2Frame::from_message(response_msg.clone(), msg_type, 0, false)
                            .expect("Failed to create the frame"),
                    );
                    downstream_sender.send(message).await.unwrap();
                }
            }
        });
    }

    pub fn next_message_from_downstream(&self) -> Option<(MsgType, AnyMessage<'static>)> {
        self.messages_from_dowsntream.next_message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{start_template_provider, template_provider::DifficultyLevel};
    use std::{convert::TryInto, net::TcpListener};
    use stratum_common::roles_logic_sv2::{
        codec_sv2::{StandardEitherFrame, Sv2Frame},
        common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess, *},
        parsers_sv2::CommonMessages,
    };

    #[tokio::test]
    async fn test_mock_downstream() {
        let (_tp, socket) = start_template_provider(None, DifficultyLevel::Low);
        let mock_downstream = MockDownstream::new(socket);
        let send_to_upstream = mock_downstream.start().await;
        let setup_connection =
            AnyMessage::Common(CommonMessages::SetupConnection(SetupConnection {
                protocol: Protocol::TemplateDistributionProtocol,
                min_version: 2,
                max_version: 2,
                flags: 0,
                endpoint_host: b"0.0.0.0".to_vec().try_into().unwrap(),
                endpoint_port: 8081,
                vendor: b"Bitmain".to_vec().try_into().unwrap(),
                hardware_version: b"901".to_vec().try_into().unwrap(),
                firmware: b"abcX".to_vec().try_into().unwrap(),
                device_id: b"89567".to_vec().try_into().unwrap(),
            }));
        let message = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
            Sv2Frame::from_message(setup_connection, MESSAGE_TYPE_SETUP_CONNECTION, 0, false)
                .expect("Failed to create the frame"),
        );
        send_to_upstream.send(message).await.unwrap();
        mock_downstream
            .messages_from_upstream
            .has_message_type(MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
    }

    #[tokio::test]
    async fn test_mock_upstream() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let upstream_socket_addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mock_downstream = MockDownstream::new(upstream_socket_addr);
        let upon_receiving_setup_connection = MESSAGE_TYPE_SETUP_CONNECTION;
        let respond_with_success = AnyMessage::Common(CommonMessages::SetupConnectionSuccess(
            SetupConnectionSuccess {
                used_version: 2,
                flags: 0,
            },
        ));
        let mock_upstream = MockUpstream::new(
            upstream_socket_addr,
            vec![(upon_receiving_setup_connection, respond_with_success)],
        );
        mock_upstream.start().await;
        let send_to_upstream = mock_downstream.start().await;
        let setup_connection =
            AnyMessage::Common(CommonMessages::SetupConnection(SetupConnection {
                protocol: Protocol::TemplateDistributionProtocol,
                min_version: 2,
                max_version: 2,
                flags: 0,
                endpoint_host: b"0.0.0.0".to_vec().try_into().unwrap(),
                endpoint_port: 8081,
                vendor: b"Bitmain".to_vec().try_into().unwrap(),
                hardware_version: b"901".to_vec().try_into().unwrap(),
                firmware: b"abcX".to_vec().try_into().unwrap(),
                device_id: b"89567".to_vec().try_into().unwrap(),
            }));
        let message = StandardEitherFrame::<AnyMessage<'_>>::Sv2(
            Sv2Frame::from_message(setup_connection, MESSAGE_TYPE_SETUP_CONNECTION, 0, false)
                .expect("Failed to create the frame"),
        );
        send_to_upstream.send(message).await.unwrap();
        mock_downstream
            .messages_from_upstream
            .has_message_type(MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS);
    }
}
