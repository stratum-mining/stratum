mod messages;
mod node;
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use std::{env, time};

const ADDR: &str = "127.0.0.1:34254";

async fn server_pool() {
    let listner = TcpListener::bind(ADDR).await.unwrap();
    let mut incoming = listner.incoming();
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

async fn new_client(name: String, test_count: u32) {
    let stream = loop {
        match TcpStream::connect(ADDR).await {
            Ok(st) => break st,
            Err(_) => {
                println!("Server not ready... retry");
                continue;
            }
        }
    };
    let client = node::Node::new(name, stream, test_count);
    task::block_on(async move {
        let mut client = client.lock().await;
        client.send_ping().await;
    });
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let test_count = if args.len() > 1 {
        args[1].parse::<u32>().unwrap()
    } else {
        u32::MAX
    };

    std::thread::spawn(|| {
        task::spawn(async {
            server_pool().await;
        });
    });
    task::block_on(async {
        let mut i: u32 = 0;
        loop {
            if i < 1 {
                new_client(format!("Client{}", i), test_count).await;
                i += 1;
            };
            task::sleep(time::Duration::from_millis(1000)).await;
        }
    });
}
