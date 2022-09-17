use roles_logic_sv2::{
    utils::{ Mutex},
};
use codec_sv2::{StandardSv2Frame,
    StandardEitherFrame
};
use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::template_distribution::ParseServerTemplateDistributionMessages,
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{NewTemplate, SetNewPrevHash, SubmitSolution},
    
};
use tokio::task;
//use messages_sv2::parsers::JobNegotiation;
pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
use async_channel::{Receiver, Sender};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use network_helpers::plain_connection_tokio::PlainConnection;
use tokio::net::TcpStream;
mod message_handler;
mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
    ) {

        let stream = TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();

        println!("TP CONNECTED")
    }
}

