use crate::{os_command, Command};
use async_channel::{Receiver, Sender, bounded};
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    HandshakeRole, Initiator, Responder, StandardEitherFrame as EitherFrame,
};
use network_helpers::{
    noise_connection_tokio::Connection, plain_connection_tokio::PlainConnection, sv1_connection_tokio::Sv1Connection
};
use std::{net::SocketAddr, clone};
use tokio::{net::{TcpListener, TcpStream}, io::{BufStream, AsyncBufReadExt, AsyncWriteExt, AsyncReadExt, self}};
use std::{time, time::Duration};

use async_std::{
    io::BufReader,
    prelude::*,
    task,
    sync::{Arc, Mutex}
};

pub async fn setup_as_upstream<
    'a,
    Message: Serialize + Deserialize<'a> + GetSize + Send + 'static,
>(
    socket: SocketAddr,
    keys: Option<(EncodedEd25519PublicKey, EncodedEd25519SecretKey)>,
    execution_commands: Vec<Command>,
    childs: &mut Vec<Option<tokio::process::Child>>,
) -> (Receiver<EitherFrame<Message>>, Sender<EitherFrame<Message>>) {
    let listner = TcpListener::bind(socket).await.unwrap();
    for command in execution_commands {
        let child = os_command(
            &command.command,
            command.args.iter().map(String::as_str).collect(),
            command.conditions,
        )
        .await;
        childs.push(child);
    }
    let (stream, _) = listner.accept().await.unwrap();
    match keys {
        Some((publ, secret)) => {
            let responder = Responder::from_authority_kp(
                publ.into_inner().as_bytes(),
                secret.into_inner().as_bytes(),
                std::time::Duration::from_secs(6000),
            )
            .unwrap();
            Connection::new(stream, HandshakeRole::Responder(responder)).await
        }
        None => PlainConnection::new(stream).await,
    }
}

pub async fn setup_as_downstream<
    'a,
    Message: Serialize + Deserialize<'a> + GetSize + Send + 'static,
>(
    socket: SocketAddr,
    key: Option<EncodedEd25519PublicKey>,
) -> (Receiver<EitherFrame<Message>>, Sender<EitherFrame<Message>>) {
    let stream = TcpStream::connect(socket).await.unwrap();
    match key {
        Some(publ) => {
            let initiator = Initiator::from_raw_k(*publ.into_inner().as_bytes()).unwrap();
            Connection::new(stream, HandshakeRole::Initiator(initiator)).await
        }
        None => PlainConnection::new(stream).await,
    }
}

pub async fn setup_as_sv1_downstream(socket: SocketAddr) -> (Receiver<String>, Sender<String>){
    // let socket = loop {
    //     tokio::time::sleep(Duration::from_secs(1)).await;
    //     match TcpStream::connect(socket).await {
    //         Ok(st) => {
    //             println!("CLIENT - connected to server at {}", socket);
    //             break st;
    //         }
    //         Err(_) => {
    //             println!("Server not ready... retry");
    //             continue;
    //         }
    //     }
    // };
    let stream = TcpStream::connect(socket).await.unwrap();
    Sv1Connection::new(stream).await
}