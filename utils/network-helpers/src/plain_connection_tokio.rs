use async_channel::{bounded, Receiver, Sender};
use binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task,
};

use binary_sv2::GetSize;
use codec_sv2::{StandardDecoder, StandardEitherFrame};
use tracing::error;

#[derive(Debug)]
pub struct PlainConnection {}

impl PlainConnection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        let (mut reader, mut writer) = stream.into_split();

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(10); // TODO caller should provide this param
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(10); // TODO caller should provide this param

        // RECEIVE AND PARSE INCOMING MESSAGES FROM TCP STREAM
        task::spawn(async move {
            let mut decoder = StandardDecoder::<Message>::new();

            loop {
                let writable = decoder.writable();
                match reader.read_exact(writable).await {
                    Ok(_) => {
                        if let Ok(x) = decoder.next_frame() {
                            sender_incoming.send(x.into()).await.unwrap();
                        }
                    }
                    Err(e) => {
                        // Just fail and force to reinitialize everything
                        error!("Failed to read from stream: {}", e);
                        break;
                    }
                }
            }
        });

        // ENCODE AND SEND INCOMING MESSAGES TO TCP STREAM
        task::spawn(async move {
            let mut encoder = codec_sv2::Encoder::<Message>::new();

            loop {
                let received = receiver_outgoing.recv().await;
                match received {
                    Ok(frame) => {
                        let b = encoder.encode(frame.try_into().unwrap()).unwrap();

                        match (&mut writer).write_all(b).await {
                            Ok(_) => (),
                            Err(_) => {
                                let _ = writer.shutdown().await;
                            }
                        }
                    }
                    Err(_) => {
                        // Just fail and force to reinitilize everything
                        let _ = writer.shutdown().await;
                        break;
                    }
                };
            }
        });

        (receiver_incoming, sender_outgoing)
    }
}

pub async fn plain_listen(address: &str, sender: Sender<TcpStream>) {
    let listner = TcpListener::bind(address).await.unwrap();
    loop {
        if let Ok((stream, _)) = listner.accept().await {
            let _ = sender.send(stream).await;
        }
    }
}
pub async fn plain_connect(address: &str) -> Result<TcpStream, ()> {
    let stream = TcpStream::connect(address).await.map_err(|_| ())?;
    Ok(stream)
}
