use async_channel::{bounded, Receiver, Sender};
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::*,
    task,
};
use binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;
use tracing::error;

use binary_sv2::GetSize;
use codec_sv2::{StandardDecoder, StandardEitherFrame};

#[derive(Debug)]
pub struct PlainConnection {}

impl PlainConnection {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
        capacity: usize,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        let (mut reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);
        let (sender_outgoing, receiver_outgoing): (
            Sender<StandardEitherFrame<Message>>,
            Receiver<StandardEitherFrame<Message>>,
        ) = bounded(capacity);

        // RECEIVE AND PARSE INCOMING MESSAGES FROM TCP STREAM
        task::spawn(async move {
            let mut decoder = StandardDecoder::<Message>::new();

            loop {
                let writable = decoder.writable();
                match reader.read_exact(writable).await {
                    Ok(_) => match decoder.next_frame() {
                        Ok(x) => {
                            if sender_incoming.send(x.into()).await.is_err() {
                                error!("Shutting down stream reader!");
                                task::yield_now().await;
                                break;
                            }
                        }
                        Err(e) => {
                            if let codec_sv2::Error::MissingBytes(_) = e {
                            } else {
                                error!("Shutting down stream reader! {:#?}", e);
                                let _ = reader.shutdown(async_std::net::Shutdown::Both);
                                break;
                            }
                        }
                    },
                    Err(_) => {
                        let _ = reader.shutdown(async_std::net::Shutdown::Both);
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

                        match (&writer).write_all(b).await {
                            Ok(_) => (),
                            Err(_) => {
                                let _ = writer.shutdown(async_std::net::Shutdown::Both);
                            }
                        }
                    }
                    Err(_) => {
                        let _ = writer.shutdown(async_std::net::Shutdown::Both);
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
    let mut incoming = listner.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        let _ = sender.send(stream).await;
    }
}
pub async fn plain_connect(address: &str) -> Result<TcpStream, ()> {
    let stream = TcpStream::connect(address).await.map_err(|_| ())?;
    Ok(stream)
}
