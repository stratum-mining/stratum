use async_channel::{Receiver, Sender};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};
use network_helpers::plain_connection_tokio::PlainConnection;
use roles_logic_sv2::{
    handlers::common::{ParseDownstreamCommonMessages, SendTo as SendToCommon},
    parsers::PoolMessages,
    utils::Mutex,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, task};

pub mod downstream_mining_node;
pub mod downstream_mining_node_status;
pub use downstream_mining_node::DownstreamMiningNode;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub async fn listen_for_downstream_mining(address: SocketAddr) {
    let listner = TcpListener::bind(address).await.unwrap();

    while let Ok((stream, _)) = listner.accept().await {
        let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;
        let node = DownstreamMiningNode::new(receiver, sender);

        task::spawn(async move {
            let mut incoming: StdFrame = node.receiver.recv().await.unwrap().try_into().unwrap();
            let message_type = incoming.get_header().unwrap().msg_type();
            let payload = incoming.payload();
            let routing_logic = crate::get_common_routing_logic();
            let node = Arc::new(Mutex::new(node));

            // Call handle_setup_connection or fail
            match DownstreamMiningNode::handle_message_common(
                node.clone(),
                message_type,
                payload,
                routing_logic,
            ) {
                Ok(SendToCommon::RelayNewMessage(_, message)) => {
                    let message = match message {
                        roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m) => m,
                        _ => panic!(),
                    };
                    DownstreamMiningNode::start(node, message).await
                }
                _ => panic!(),
            }
        });
    }
}
