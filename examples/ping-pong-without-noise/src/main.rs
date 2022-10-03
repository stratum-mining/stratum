mod messages;
mod node;
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use std::{env, net::SocketAddr, thread::sleep, time};

//Pick any unused port
const ADDR: &str = "127.0.0.1:0";

async fn server_pool_listen(listener: TcpListener) {
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let _server = node::Node::new(
            "server".to_string(),
            stream,
            u32::MAX, //We only need the client to have a valid test count
        );
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

    let client = node::Node::new(name, stream, test_count);
    task::block_on(async move {
        loop {
            if let Some(mut client) = client.try_lock() {
                client.send_ping().await;
                break;
            }
            sleep(time::Duration::from_millis(500));
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
                println!("Client connecting");
                new_client(format!("Client{}", i), test_count, socket).await;
                i += 1;
            };
            task::sleep(time::Duration::from_millis(1000)).await;
        }
    });
}
