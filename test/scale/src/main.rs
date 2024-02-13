use std::thread;
use tokio::{
    net::{TcpListener, TcpStream},
    task,
};

use async_channel::{bounded, Receiver, Sender};

use clap::{App, Arg};
use codec_sv2::{HandshakeRole, Initiator, Responder, StandardEitherFrame, StandardSv2Frame};
use std::time::Duration;

use network_helpers::{
    noise_connection_tokio::Connection, plain_connection_tokio::PlainConnection,
};

use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::{
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages},
};

pub type EitherFrame = StandardEitherFrame<Message>;
pub const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";

pub const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";

static HOST: &str = "127.0.0.1";

#[tokio::main]
async fn main() {
    let matches = App::new("ScaleTest")
        .arg(Arg::with_name("encrypt").short("e").help("Use encryption"))
        .arg(
            Arg::with_name("hops")
                .short("h")
                .takes_value(true)
                .help("Number of hops"),
        )
        .get_matches();

    let total_messages = 1_000_000;
    let encrypt = matches.is_present("encrypt");
    let hops: u16 = matches.value_of("hops").unwrap_or("0").parse().unwrap_or(0);
    let mut orig_port: u16 = 19000;

    // create channel to tell final server number of messages
    let (tx, rx) = bounded(1);

    if hops > 0 {
        orig_port = spawn_proxies(encrypt, hops, tx, total_messages).await;
    } else {
        println!("Usage: ./program -h <hops> -e");
    }
    println!("Connecting to localhost:{}", orig_port);
    setup_driver(orig_port, encrypt, rx, total_messages, hops).await;
}

async fn setup_driver(
    server_port: u16,
    encrypt: bool,
    rx: Receiver<String>,
    total_messages: i32,
    hops: u16,
) {
    let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port))
        .await
        .unwrap();
    let (_server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);

    if encrypt {
        let k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
        let initiator = Initiator::from_raw_k(k.into_bytes()).unwrap();

        (_, server_sender, _, _) =
            Connection::new(server_stream, HandshakeRole::Initiator(initiator))
                .await
                .unwrap();
    } else {
        (_server_receiver, server_sender) = PlainConnection::new(server_stream).await;
    }
    // Create timer to see how long this method takes
    let start = std::time::Instant::now();

    send_messages(server_sender, total_messages).await;

    //listen for message on rx
    let msg = rx.recv().await.unwrap();

    let end = std::time::Instant::now();

    println!(
        "client: {} - Took {}s hops: {} encryption: {}",
        msg,
        (end - start).as_secs(),
        hops,
        encrypt
    );
}

pub type Message = MiningDeviceMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;

async fn send_messages(stream: Sender<EitherFrame>, total_messages: i32) {
    let mut number: i32 = 0;
    println!("Creating share");
    let share = MiningDeviceMessages::Mining(Mining::SubmitSharesStandard(SubmitSharesStandard {
        channel_id: 1,
        sequence_number: number as u32,
        job_id: 2,
        nonce: 3,
        ntime: 4,
        version: 5,
    }));

    while number <= total_messages {
        //println!("client: sending msg-{}", number);
        let frame: StdFrame = share.clone().try_into().unwrap();
        let binary: EitherFrame = frame.into();

        stream.send(binary).await.unwrap();
        number += 1;
    }
}

async fn handle_messages(
    _name: String,
    client: Receiver<EitherFrame>,
    server: Option<Sender<EitherFrame>>,
    total_messages: i32,
    tx: Sender<String>,
) {
    let mut messages_received = 0;

    while messages_received <= total_messages {
        let frame: StdFrame = client.recv().await.unwrap().try_into().unwrap();

        let binary: EitherFrame = frame.into();

        if server.is_some() {
            server.as_ref().unwrap().send(binary).await.unwrap();
        } else {
            messages_received += 1;
            //println!("last server: {} got msg {}", name, messages_received);
        }
    }
    tx.send("got all messages".to_string()).await.unwrap();
}

async fn create_proxy(
    name: String,
    listen_port: u16,
    server_port: u16,
    encrypt: bool,
    total_messages: i32,
    tx: Sender<String>,
) {
    println!(
        "Creating proxy listener {}: {} connecting to: {}",
        name, listen_port, server_port
    );
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port))
        .await
        .unwrap();
    println!("Bound - now waiting for connection...");
    let cli_stream = listener.accept().await.unwrap().0;
    let (cli_receiver, _cli_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);

    if encrypt {
        let k_pub: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
        let k_priv: Secp256k1SecretKey = AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();
        let responder = Responder::from_authority_kp(
            &k_pub.into_bytes(),
            &k_priv.into_bytes(),
            Duration::from_secs(3600),
        )
        .unwrap();
        (cli_receiver, _, _, _) = Connection::new(cli_stream, HandshakeRole::Responder(responder))
            .await
            .unwrap();
    } else {
        (cli_receiver, _cli_sender) = PlainConnection::new(cli_stream).await;
    }

    let mut server = None;
    if server_port > 0 {
        println!("Proxy {} Connecting to server: {}", name, server_port);
        let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port))
            .await
            .unwrap();
        let (_server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);
        let k_pub: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();

        if encrypt {
            let initiator = Initiator::from_raw_k(k_pub.into_bytes()).unwrap();
            (_, server_sender, _, _) =
                Connection::new(server_stream, HandshakeRole::Initiator(initiator))
                    .await
                    .unwrap();
        } else {
            (_server_receiver, server_sender) = PlainConnection::new(server_stream).await;
        }
        server = Some(server_sender);
    }

    println!("Proxy {} has a client", name);
    handle_messages(name, cli_receiver, server, total_messages, tx).await;
}

async fn spawn_proxies(encrypt: bool, hops: u16, tx: Sender<String>, total_messages: i32) -> u16 {
    let orig_port: u16 = 19000;
    let final_server_port = orig_port + (hops - 1);
    let mut listen_port = final_server_port;
    let mut server_port: u16 = 0;

    for name in (0..hops).rev() {
        let tx_clone = tx.clone();
        let name_clone = name.to_string();

        task::spawn(async move {
            create_proxy(
                name_clone,
                listen_port,
                server_port,
                encrypt,
                total_messages,
                tx_clone,
            )
            .await;
        });

        thread::sleep(std::time::Duration::from_secs(1));
        server_port = listen_port;
        listen_port -= 1;
    }
    orig_port
}
