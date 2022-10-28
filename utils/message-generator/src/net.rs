use async_channel::{Receiver, Sender};
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    HandshakeRole, Initiator, Responder, StandardEitherFrame as EitherFrame,
};
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
    keys: Option<(EncodedEd25519PublicKey, EncodedEd25519SecretKey)>,
) -> (Receiver<EitherFrame<Message>>, Sender<EitherFrame<Message>>) {
    let listner = TcpListener::bind(socket).await.unwrap();
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
