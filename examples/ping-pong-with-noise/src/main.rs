mod messages;
mod node;
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use codec_sv2::{HandshakeRole, Initiator, Responder};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use std::{convert::TryInto, env, net::SocketAddr, time};

//Pick any unused port
const ADDR: &str = "127.0.0.1:0";

pub const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";

pub const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";

const CERT_VALIDITY: time::Duration = time::Duration::from_secs(3600);

async fn server_pool_listen(listener: TcpListener) {
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let k_pub: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
        let k_priv: Secp256k1SecretKey = AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();
        let responder =
            Responder::from_authority_kp(&k_pub.into_bytes(), &k_priv.into_bytes(), CERT_VALIDITY)
                .unwrap();
        let _server = node::Node::new(
            "server".to_string(),
            stream,
            HandshakeRole::Responder(responder),
            u32::MAX, //We only need the client to have a valid test count
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
    let k_pub: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
    let initiator = Initiator::from_raw_k(k_pub.into_bytes()).unwrap();
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
