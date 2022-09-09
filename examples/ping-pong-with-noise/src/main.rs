mod messages;
mod node;
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use codec_sv2::{HandshakeRole, Initiator, Responder};
use std::{env, net::SocketAddr, time};

//Pick any unused port
const ADDR: &str = "127.0.0.1:0";

pub const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

pub const AUTHORITY_PRIVATE_K: [u8; 32] = [
    204, 93, 167, 220, 169, 204, 172, 35, 9, 84, 174, 208, 171, 89, 25, 53, 196, 209, 161, 148, 4,
    5, 173, 0, 234, 59, 15, 127, 31, 160, 136, 131,
];

const CERT_VALIDITY: time::Duration = time::Duration::from_secs(3600);

async fn server_pool_listen(listener: TcpListener) {
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let responder = Responder::from_authority_kp(
            &AUTHORITY_PUBLIC_K[..],
            &AUTHORITY_PRIVATE_K[..],
            CERT_VALIDITY,
        )
        .unwrap();
        let _server = node::Node::new(
            "server".to_string(),
            stream,
            HandshakeRole::Responder(responder),
            u32::MAX
        )
        .await;
    }
}

async fn new_client(name: String, test_count: u32, socket: SocketAddr) {
    let stream = loop {
        match TcpStream::connect(socket).await {
            Ok(st) => break st,
            Err(_) => {
                println!("Server not ready... retry");
                continue;
            }
        }
    };
    let initiator = Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap();
    let client = node::Node::new(
        name,
        stream,
        HandshakeRole::Initiator(initiator),
        test_count,
    )
    .await;

    task::block_on(async move {
        loop {
            if let Some(mut client) = client.try_lock() {
                client.send_pong().await;
                break;
            }
        }
    });
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let test_count = if args.len() > 1 {
        args[1].parse::<u32>().unwrap()
    } else {
        u32::MAX
    };

    //Listen on available port and wait for bind
    let listener = task::block_on(async move {
        let listener = TcpListener::bind(ADDR).await.unwrap();
        println!("Server listening on: {}", listener.local_addr().unwrap());
        listener
    });

    let socket = listener.local_addr().unwrap();

    std::thread::spawn(|| {
        task::spawn(async move {
            server_pool_listen(listener).await;
        });
    });

    task::block_on(async {
        let mut i: u32 = 0;
        loop {
            if i < 1 {
                new_client(format!("Client{}", i), test_count, socket).await;
                i += 1;
            };
            task::sleep(time::Duration::from_millis(1000)).await;
        }
    });
}
