use async_channel::{bounded, Receiver, Sender};
use codec_sv2::binary_sv2::{Deserialize, Serialize};
use core::convert::TryInto;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task,
};

use codec_sv2::{binary_sv2::GetSize, Error::MissingBytes, StandardDecoder, StandardEitherFrame};
use tracing::{error, trace};

#[derive(Debug)]
pub struct PlainConnection {}

impl PlainConnection {
    ///
    ///
    /// # Arguments
    ///
    /// * `strict` - true - will disconnect a connection that sends a message that can't be
    ///   translated, false - will ignore messages that can't be translated
    #[allow(clippy::new_ret_no_self)]
    pub async fn new<'a, Message: Serialize + Deserialize<'a> + GetSize + Send + 'static>(
        stream: TcpStream,
    ) -> (
        Receiver<StandardEitherFrame<Message>>,
        Sender<StandardEitherFrame<Message>>,
    ) {
        const NOISE_HANDSHAKE_SIZE_HINT: usize = 3363412;

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
                        match decoder.next_frame() {
                            Ok(frame) => {
                                if let Err(e) = sender_incoming.send(frame.into()).await {
                                    error!("Failed to send incoming message: {}", e);
                                    task::yield_now().await;
                                    break;
                                }
                            }
                            Err(MissingBytes(size)) => {
                                // Only disconnect if we get noise handshake message - this
                                // shouldn't
                                // happen in plain_connection
                                if size == NOISE_HANDSHAKE_SIZE_HINT {
                                    error!("Got noise message on unencrypted connection - disconnecting");
                                    break;
                                } else {
                                    trace!("MissingBytes({}) on incoming message - ignoring", size);
                                }
                            }
                            Err(e) => {
                                error!("Failed to read from stream: {}", e);
                                sender_incoming.close();
                                task::yield_now().await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        // Just fail and force to reinitialize everything
                        error!("Failed to read from stream: {}", e);
                        sender_incoming.close();
                        task::yield_now().await;
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

                        match (writer).write_all(b).await {
                            Ok(_) => (),
                            Err(_) => {
                                let _ = writer.shutdown().await;
                            }
                        }
                    }
                    Err(_) => {
                        // Just fail and force to reinitilize everything
                        let _ = writer.shutdown().await;
                        error!("Failed to read from stream - terminating connection");
                        task::yield_now().await;
                        break;
                    }
                };
            }
        });

        (receiver_incoming, sender_outgoing)
    }
}

pub async fn plain_listen(address: &str, sender: Sender<TcpStream>) {
    let listener = TcpListener::bind(address).await.unwrap();
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = sender.send(stream).await;
        }
    }
}
pub async fn plain_connect(address: &str) -> Result<TcpStream, ()> {
    let stream = TcpStream::connect(address).await.map_err(|_| ())?;
    Ok(stream)
}
