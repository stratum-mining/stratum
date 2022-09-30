mod executor;
mod net;

use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    Frame, StandardEitherFrame as EitherFrame, Sv2Frame,
};
use net::{setup_as_downstream, setup_as_upstream};
use roles_logic_sv2::{
    mining_sv2::{CloseChannel, SetTarget},
    parsers::Mining,
};
use std::net::SocketAddr;

#[derive(Debug,PartialEq, Eq,Clone)]
enum Sv2Type {
    Bool(bool),
    U8(u8),
    U16(u16),
    U24(Vec<u8>),
    U32(u32),
    U256(Vec<u8>),
    Str0255(String),
    B0255(Vec<u8>),
    B064K(Vec<u8>),
    B016m(Vec<u8>),
    Pubkey(Vec<u8>),
    Seq0255(Vec<Vec<u8>>),
    Seq064k(Vec<Vec<u8>>),
}

#[derive(Debug,PartialEq, Eq,Clone)]
enum ActionResult {
    MatchMessageType(u8),
    MatchMessageField(Sv2Type),
    MatchMessageLen(usize),
    MatchExtensionType(u16),
    CloseConnection,
    None,
}

#[derive(Debug,PartialEq, Eq,Clone)]
enum Role {
    Upstream,
    Downstream,
}

#[derive(Debug,Clone)]
struct Upstream {
    addr: SocketAddr,
    keys: Option<(EncodedEd25519PublicKey,EncodedEd25519SecretKey)>,
}

#[derive(Debug,Clone)]
struct Downstream {
    addr: SocketAddr,
    key: Option<EncodedEd25519PublicKey>,
}

#[derive(Debug)]
struct Action<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    messages: Vec<EitherFrame<Message>>,
    result: ActionResult,
    role: Role,
}

#[derive(Debug)]
pub struct Test<Message: Serialize + Deserialize<'static> + GetSize + Send  + 'static> {
    actions: Vec<Action<Message>>,
    as_upstream: Option<Upstream>,
    as_dowstream: Option<Downstream>,
    // TODO setup_commmands: commands,
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    use super::*;
    use std::convert::TryInto;
    use tokio::join;

    #[tokio::test]
    async fn it_send_and_receive() {
        let message = CloseChannel {
            channel_id: 78,
            reason_code: "no reason".to_string().try_into().unwrap(),
        };
        let frame = Sv2Frame::from_message(
            message.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let server_socket = SocketAddr::new("127.0.0.1".parse().unwrap(), 34254);
        let client_socket = SocketAddr::new("127.0.0.1".parse().unwrap(), 34254);
        let ((server_recv, server_send), (client_recv, client_send)) = join!(
            setup_as_upstream(server_socket, None),
            setup_as_downstream(client_socket, None)
        );
        server_send
            .send(frame.clone().try_into().unwrap())
            .await
            .unwrap();
        client_send
            .send(frame.clone().try_into().unwrap())
            .await
            .unwrap();
        let server_received = server_recv.recv().await.unwrap();
        let client_received = client_recv.recv().await.unwrap();
        match (server_received, client_received) {
            (EitherFrame::Sv2(mut frame1), EitherFrame::Sv2(mut frame2)) => {
                let mt1 = frame1.get_header().unwrap().msg_type();
                let mt2 = frame2.get_header().unwrap().msg_type();
                let p1 = frame1.payload();
                let p2 = frame2.payload();
                let message1: Mining = (mt1, p1).try_into().unwrap();
                let message2: Mining = (mt2, p2).try_into().unwrap();
                match (message1, message2) {
                    (Mining::CloseChannel(message1), Mining::CloseChannel(message2)) => {
                        assert!(message1.channel_id == message2.channel_id);
                        assert!(message2.channel_id == message.channel_id);
                        assert!(message1.reason_code == message2.reason_code);
                        assert!(message2.reason_code == message.reason_code);
                    }
                    _ => assert!(false),
                }
            }
            _ => assert!(false),
        }
    }

    #[test]
    fn it_create_tests_with_different_messages() {
        let message1 = CloseChannel {
            channel_id: 78,
            reason_code: "no reason".to_string().try_into().unwrap(),
        };
        let maximum_target: binary_sv2::U256 = [0;32].try_into().unwrap();
        let message2 = SetTarget {
            channel_id: 78,
            maximum_target,
        };
        let message1 = Mining::CloseChannel(message1);
        let message2 = Mining::SetTarget(message2);
        let frame = Sv2Frame::from_message(
            message1.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let frame = EitherFrame::Sv2(frame);
        let frame2 = Sv2Frame::from_message(
            message2.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let frame2 = EitherFrame::Sv2(frame2);
        let _ = vec![frame,frame2];
        assert!(true)
    }
}
