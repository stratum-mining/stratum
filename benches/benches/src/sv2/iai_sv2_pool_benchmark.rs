use async_channel::{Receiver, Sender};
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::MiningDeviceMessages;
use iai::{black_box, main};
use async_std::net::TcpStream;
use codec_sv2::{HandshakeRole, Initiator};
use async_std::task;
use network_helpers::Connection;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[path = "./lib/client.rs"]
mod client;
use crate::client::Device;

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];
async fn share_submit () {
async_std::task::block_on(async {
    let addr: SocketAddr =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 34254);
    let stream = TcpStream::connect(addr).await.unwrap();
    let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        Connection::new(
            stream,
            HandshakeRole::Initiator(
                Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap(),
            ),
            10,
        )
        .await;

    let handicap: u32 = 10;
    Device::share_submission(addr, receiver.clone(), sender.clone(), handicap).await

});
}
fn iai_share_submit() {
    black_box(task::block_on(share_submit()))
}
main!(iai_share_submit);
