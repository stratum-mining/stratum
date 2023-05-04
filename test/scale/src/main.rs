use tokio::{
    task,
    net::{TcpListener, TcpStream}};
use std::thread;

use async_channel::{bounded, Receiver, Sender};

use clap::{Arg, App};
use std::time::Duration;
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{HandshakeRole, Initiator, Responder,
                StandardSv2Frame, StandardEitherFrame
};

use network_helpers::plain_connection_tokio::PlainConnection;
use network_helpers::noise_connection_tokio::Connection;

use roles_logic_sv2::{
    mining_sv2::*,
    parsers::{Mining, MiningDeviceMessages},
};

pub type EitherFrame = StandardEitherFrame<Message>;


pub const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

pub const AUTHORITY_PRIVATE_K: [u8; 32] = [
    204, 93, 167, 220, 169, 204, 172, 35, 9, 84, 174, 208, 171, 89, 25, 53, 196, 209, 161, 148, 4,
    5, 173, 0, 234, 59, 15, 127, 31, 160, 136, 131,
];

static HOST: &'static str = "127.0.0.1";

#[tokio::main]
async fn main() {
    let matches = App::new("ScaleTest")
        .arg(Arg::with_name("encrypt")
            .short("e")
            .help("Use encryption"))
        .arg(Arg::with_name("hops")
            .short("h")
            .takes_value(true)
            .help("Number of hops"))
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

async fn setup_driver(server_port: u16, encrypt: bool, rx: Receiver<String>, total_messages: i32,
    hops: u16) {
    let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port)).await.unwrap();
    let (_server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);

    if encrypt {
        let initiator = Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap();

        (_server_receiver, server_sender) = Connection::new(server_stream, HandshakeRole::Initiator(initiator)).await;
    } else {
        (_server_receiver, server_sender) = PlainConnection::new(server_stream).await;
    }
    // Create timer to see how long this method takes
    let start = std::time::Instant::now();

    send_messages(server_sender, total_messages).await;

    //listen for message on rx
    let msg = rx.recv().await.unwrap();

    let end = std::time::Instant::now();

    println!("client: {} - Took {}s hops: {} encryption: {}", msg, (end - start).as_secs(),
        hops, encrypt);

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
        println!("client: sending msg-{}", number);
        let frame: StdFrame = share.clone().try_into().unwrap();
        let binary: EitherFrame = frame.into();

        stream.send(binary).await.unwrap();
        number = number + 1;
    }
}

async fn handle_messages<Mining: Serialize + Deserialize<'static> + GetSize + Send + 'static>(
    name: String, client: Receiver<EitherFrame>, server: Option<Sender<EitherFrame>>,
    total_messages: i32, tx: Sender<String>) {

    let mut messages_received = 0;

    while messages_received <= total_messages {
        let frame: StdFrame = client.recv().await.unwrap().try_into().unwrap();

        let binary: EitherFrame = frame.into();

        if server.is_some() {
            server.as_ref().unwrap().send(binary).await.unwrap();
        } else {
            messages_received = messages_received + 1;
            println!("last server: {} got msg {}", name, messages_received);
        }

    }
    tx.send("got all messages".to_string()).await.unwrap();
}

async fn create_proxy(name: String, listen_port: u16, server_port: u16, encrypt: bool, total_messages: i32, tx: Sender<String>) {
    println!("Creating proxy listener {}: {} connecting to: {}", name, listen_port.to_string(), server_port.to_string());
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await.unwrap();
    println!("Bound - now waiting for connection...");
    let cli_stream = listener.accept().await.unwrap().0;
    let (cli_receiver, _cli_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);

    if encrypt {
        let responder = Responder::from_authority_kp(
            &AUTHORITY_PUBLIC_K[..],
            &AUTHORITY_PRIVATE_K[..],
            Duration::from_secs(3600),
        )
            .unwrap();
        (cli_receiver, _cli_sender) = Connection::new(cli_stream, HandshakeRole::Responder(responder)).await;
    } else {
        (cli_receiver, _cli_sender) = PlainConnection::new(cli_stream).await;
    }

    let mut server = None;
    if server_port > 0 {
        println!("Proxy {} Connecting to server: {}", name, server_port.to_string());
        let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port)).await.unwrap();
        let (_server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>);

        if encrypt {
            let initiator = Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap();
            (_server_receiver, server_sender) = Connection::new(server_stream,
                                                                HandshakeRole::Initiator(initiator)).await;
        } else {
            (_server_receiver, server_sender) = PlainConnection::new(server_stream).await;
        }
        server = Some(server_sender);
    }

    println!("Proxy {} has a client", name);
    handle_messages::<Mining>(name, cli_receiver, server, total_messages, tx).await;

}


async fn spawn_proxies(encrypt: bool, hops: u16, tx: Sender<String>, total_messages: i32) -> u16 {
    let orig_port : u16 = 19000;
    let final_server_port = orig_port + (hops - 1);
    let mut listen_port = final_server_port;
    let mut server_port: u16 = 0;

    for name in (0..hops).rev() {
        let tx_clone = tx.clone();
        let name_clone = name.to_string();

        task::spawn(async move {
            create_proxy(name_clone, listen_port, server_port, encrypt, total_messages, tx_clone).await;
        });

        thread::sleep(std::time::Duration::from_secs(1));
        server_port = listen_port;
        listen_port = listen_port - 1;
    }
    return orig_port;
}