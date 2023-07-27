use crate::{os_command, Command};
use async_channel::{Receiver, Sender};
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{HandshakeRole, Initiator, Responder, StandardEitherFrame as EitherFrame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers::{
    noise_connection_tokio::Connection, plain_connection_tokio::PlainConnection,
};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

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
            Connection::new(stream, HandshakeRole::Responder(responder))
                .await
                .unwrap()
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
            Connection::new(stream, HandshakeRole::Initiator(initiator))
                .await
                .unwrap()
        }
        None => PlainConnection::new(stream).await,
    }
}
