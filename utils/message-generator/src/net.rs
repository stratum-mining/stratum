use crate::{os_command, Command};
use async_channel::{bounded, Receiver, Sender};
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{HandshakeRole, Initiator, Responder, StandardEitherFrame as EitherFrame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::{
    noise_connection_tokio::Connection, plain_connection_tokio::PlainConnection,
};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task,
};

pub async fn setup_as_upstream<
    'a,
    Message: Serialize + Deserialize<'a> + GetSize + Send + 'static,
>(
    socket: SocketAddr,
    keys: Option<(Secp256k1PublicKey, Secp256k1SecretKey)>,
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
                &publ.into_bytes(),
                &secret.into_bytes(),
                std::time::Duration::from_secs(6000),
            )
            .unwrap();
            let (recv, sender, _, _) = Connection::new(stream, HandshakeRole::Responder(responder))
                .await
                .unwrap();
            (recv, sender)
        }
        None => PlainConnection::new(stream).await,
    }
}

pub async fn setup_as_downstream<
    'a,
    Message: Serialize + Deserialize<'a> + GetSize + Send + 'static,
>(
    socket: SocketAddr,
    key: Option<Secp256k1PublicKey>,
) -> (Receiver<EitherFrame<Message>>, Sender<EitherFrame<Message>>) {
    let stream = TcpStream::connect(socket).await.unwrap();
    match key {
        Some(publ) => {
            let initiator = Initiator::from_raw_k(publ.into_bytes()).unwrap();
            let (recv, sender, _, _) = Connection::new(stream, HandshakeRole::Initiator(initiator))
                .await
                .unwrap();
            (recv, sender)
        }
        None => PlainConnection::new(stream).await,
    }
}

pub async fn setup_as_sv1_downstream(socket: SocketAddr) -> (Receiver<String>, Sender<String>) {
    let socket = loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match TcpStream::connect(socket).await {
            Ok(st) => {
                println!("CLIENT - connected to server at {}", socket);
                break st;
            }
            Err(_) => {
                println!("Server not ready... retry");
                continue;
            }
        }
    };
    let (mut reader, mut writer) = socket.into_split();

    let (sender_incoming, receiver_incoming): (Sender<String>, Receiver<String>) = bounded(10);
    let (sender_outgoing, receiver_outgoing): (Sender<String>, Receiver<String>) = bounded(10);

    // RECEIVE INCOMING MESSAGES FROM TCP STREAM
    task::spawn(async move {
        loop {
            let mut buf = vec![0; 1024];
            match reader.read(&mut buf).await {
                Ok(n) => {
                    let message = String::from_utf8_lossy(&buf[..n]).to_string();
                    sender_incoming.send(message).await.unwrap();
                    buf.clear();
                }
                Err(e) => {
                    // Just fail and force to reinitialize everything
                    println!("Failed to read from stream: {}", e);
                    sender_incoming.close();
                    task::yield_now().await;
                    break;
                }
            }
        }
    });

    // SEND INCOMING MESSAGES TO TCP STREAM
    task::spawn(async move {
        loop {
            let received = receiver_outgoing.recv().await;
            match received {
                Ok(msg) => match (writer).write_all(msg.as_bytes()).await {
                    Ok(_) => (),
                    Err(_) => {
                        let _ = writer.shutdown().await;
                    }
                },
                Err(_) => {
                    // Just fail and force to reinitilize everything
                    let _ = writer.shutdown().await;
                    println!("Failed to read from stream - terminating connection");
                    task::yield_now().await;
                    break;
                }
            };
        }
    });
    (receiver_incoming, sender_outgoing)
}
