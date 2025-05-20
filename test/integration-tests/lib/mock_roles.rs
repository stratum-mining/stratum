use crate::{
    message_aggregator::MessagesAggregator,
    types::{MessageFrame, MsgType},
    utils::{create_upstream, message_from_frame},
};
use async_channel::Sender;
use roles_logic_sv2::parsers::AnyMessage;
use std::net::SocketAddr;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::start_template_provider;
    use codec_sv2::{StandardEitherFrame, Sv2Frame};
    use roles_logic_sv2::{
        common_messages_sv2::{Protocol, SetupConnection},
        parsers::CommonMessages,
    };
    use std::convert::TryInto;
    use stratum_common::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS};

    #[tokio::test]
    async fn test_mock_downstream() {
        let (_tp, socket) = start_template_provider(None);
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
}
