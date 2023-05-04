use std::io::Read;
use tokio::{
    task,
    net::{TcpListener, TcpStream}};
use std::thread;

use async_channel::{bounded, Receiver, SendError, Sender};

use clap::{Arg, App};
use std::time::Duration;
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{HandshakeRole, Initiator, Responder,
                StandardNoiseDecoder, StandardSv2Frame, StandardEitherFrame
};
use network_helpers::plain_connection_tokio::PlainConnection;

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

const CERT_VALIDITY: Duration = Duration::from_secs(3600);
#[tokio::main]
async fn main() {
    let matches = App::new("Example program")
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
        orig_port = hop_server(encrypt, hops, tx, total_messages).await;
    } else {
        println!("Usage: ./program -h <hops> -e");
    }
    println!("Connecting to localhost:{}", orig_port);
    //
    setup_driver(orig_port, encrypt, rx, total_messages).await;
}

async fn setup_driver(server_port: u16, encrypt: bool, rx: Receiver<String>, total_messages: i32) {
    let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port)).await.unwrap();

    let (server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        PlainConnection::new(server_stream).await;

    // Create timer to see how long this method takes
    let start = std::time::Instant::now();

    send_messages(server_sender, total_messages).await;

    //listen for message on rx
    let msg = rx.recv().await.unwrap();

    let end = std::time::Instant::now();

    println!("client: {} - Took {}s", msg, (end - start).as_secs());

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
        println!("{} is waiting...", name);
        let mut frame: StdFrame = client.recv().await.unwrap().try_into().unwrap();
        println!("{} got msg {}", name, messages_received);

        let binary: EitherFrame = frame.into();


        if server.is_some() {
            server.as_ref().unwrap().send(binary).await;
        } else {
            messages_received = messages_received + 1;
            println!("last server: {} got msg {}", name, messages_received);
        }

    }
    tx.send("got all messages".to_string()).await.unwrap();
}

// fn handle_messages_old<Mining: Serialize + Deserialize<'static> + GetSize + Send + 'static>(name: String, client: TcpStream, server: Option<TcpStream>, total_messages: i32, tx: Sender<String>) {
//     let mut reader = std::io::BufReader::new(client);
//
//     let responder = Responder::from_authority_kp(
//         &AUTHORITY_PUBLIC_K[..],
//         &AUTHORITY_PRIVATE_K[..],
//         CERT_VALIDITY,
//     ).unwrap();
//
//     let initiator = Initiator::from_raw_k(AUTHORITY_PUBLIC_K).unwrap();
//     let role = HandshakeRole::Initiator(initiator);
//
//     let mut decoder = StandardNoiseDecoder::<Mining>::new();
//
//     let mut messages_recieved = 0;
//
//     loop {
//         let mut buffer = decoder.writable();
//
//         let result = reader.read_exact(&mut buffer);
//
//         messages_recieved = messages_recieved + 1;
//
//         if buffer.len() > 0 {
//             if server.is_some() {
//                 server.as_ref().unwrap().write(buffer).unwrap();
//             } else {
//                 println!("server: {} got {:?}", name, buffer);
//                 // This is the last server - so when this gets the last message send the main thread
//                 //the "got it" message
//
//                 // parse buffer to i32
//                 if messages_recieved == total_messages {
//                     tx.send("got it".to_string()).unwrap();
//                 }
//             }
//         } else {
//             println!("server: {} received empty message", name);
//         }
//     }
// }

async fn create_proxy(name: String, listen_port: u16, server_port: u16, encrypt: bool, total_messages: i32, tx: Sender<String>) {
    println!("Creating proxy listener {}: {} connecting to: {}", name, listen_port.to_string(), server_port.to_string());
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await.unwrap();
    println!("Bound - now waiting for connection...");
    let cli_stream = listener.accept().await.unwrap().0;
    let (cli_receiver, cli_sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
        network_helpers::plain_connection_tokio::PlainConnection::new(cli_stream).await;

    let mut server = None;
    if server_port > 0 {
        println!("Proxy {} Connecting to server: {}", name, server_port.to_string());
        let server_stream = TcpStream::connect(format!("{}:{}", HOST, server_port)).await.unwrap();

        let (server_receiver, server_sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(server_stream).await;
        server = Some(server_sender);
    }

    println!("Proxy {} has a client", name);
    handle_messages::<Mining>(name, cli_receiver, server, total_messages, tx).await;

}


async fn hop_server(encrypt: bool, hops: u16, mut tx: Sender<String>, total_messages: i32) -> u16 {
    let orig_port : u16 = 19000;
    let final_server_port = orig_port + (hops - 1);
    let mut listen_port = final_server_port;
    let mut server_port: u16 = 0;

    for name in (0..hops).rev() {
        let tx_clone = tx.clone();
        let name_clone = name.to_string();

        //let port_clone = server_port.clone();
        task::spawn(async move {
            create_proxy(name_clone, listen_port, server_port, encrypt, total_messages, tx_clone).await;

        });

        thread::sleep(std::time::Duration::from_secs(1));
        server_port = listen_port;
        listen_port = listen_port - 1;
    }
    return orig_port;
}
// fn create_proxy_old(name: String, listen_port: u16, server_port: u16, encrypt: bool, total_messages: i32, tx: Sender<String>) {
//     println!("Creating proxy listener {}: {} connecting to: {}", name, listen_port.to_string(), server_port.to_string());
//     let listener = TcpListener::bind(format!("localhost:{}", listen_port)).await.unwrap();
//     let mut server_stream = None;
//
//     if server_port > 0 {
//         println!("Proxy {} Connecting to server: {}", name, server_port.to_string());
//         server_stream = TcpStream::connect(format!("localhost:{}", server_port)).await.ok();
//     }
//
//     let client_stream = listener.accept().await.unwrap().0;
//        println!("Proxy {} has a client", name);
//     handle_messages::<Mining>(name, client_stream, server_stream, total_messages, tx);
//
// }


// fn hop_server_old(encrypt: bool, hops: u16, mut tx: Sender<String>, total_messages: i32) -> u16 {
//     let orig_port : u16 = 19000;
//     let final_server_port = orig_port + (hops - 1);
//     let mut listen_port = final_server_port;
//     let mut server_port = 0;
//
//     for name in (0..hops).rev() {
//         let tx_clone = tx.clone();
//         thread::spawn(move || {
//             create_proxy(name.to_string(), listen_port, server_port, encrypt, total_messages, tx_clone);
//         });
//         thread::sleep(std::time::Duration::from_secs(1));
//         server_port = listen_port;
//         listen_port = listen_port - 1;
//     }
//     return orig_port;
// }
