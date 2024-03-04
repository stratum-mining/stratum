use async_channel::{Receiver, Sender};
use async_std::net::TcpStream;
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame, Sv2Frame};
use network_helpers::PlainConnection;
use roles_logic_sv2::{
    parsers::{IsSv2Message, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
};
use std::{
    convert::TryInto,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

pub type Message = TemplateDistribution<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[async_std::main]
async fn main() {
    test_1().await;
}
#[allow(unused)]

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 == 0 {
        (0..s.len())
            .step_by(2)
            .map(|i| {
                s.get(i..i + 2)
                    .and_then(|sub| u8::from_str_radix(sub, 16).ok())
            })
            .collect()
    } else {
        None
    }
}
async fn test_1() {
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8442);
    let stream = TcpStream::connect(socket).await.unwrap();

    let (_receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        PlainConnection::new(stream, 10).await;
    let submit_solution = SubmitSolution {
        template_id: 0,
        version: 0x01000000,
        header_timestamp: 0x29ab5f49,
        header_nonce: 0x1dac2b7c,
        coinbase_tx: hex_to_bytes("01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000")
            .unwrap()
            .try_into()
            .unwrap(),
    };
    let submit_solution: Message = TemplateDistribution::SubmitSolution(submit_solution);
    let extension_type = 0;
    let channel_bit = submit_solution.channel_bit();
    // Below is the right message type
    let message_type = submit_solution.message_type();
    let frame: StdFrame =
        Sv2Frame::from_message(submit_solution, message_type, extension_type, channel_bit).unwrap();
    let frame: EitherFrame = frame.into();
    sender.send(frame).await.unwrap();
    async_std::task::sleep(std::time::Duration::from_secs(10)).await;
}
