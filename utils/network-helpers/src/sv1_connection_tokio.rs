use async_channel::{bounded, Sender, Receiver};
use tokio::{net::{TcpListener, TcpStream}, io::{BufReader, AsyncBufReadExt, AsyncWriteExt, AsyncReadExt}, task};



#[derive(Debug)]
pub struct Sv1Connection {}

impl Sv1Connection {
    pub async fn new(stream: TcpStream) -> (Receiver<String>, Sender<String>){
        let (mut reader, mut writer) = stream.into_split();
        
        let (sender_incoming, receiver_incoming): (Sender<String>, Receiver<String>) = bounded(10);
        let (sender_outgoing, receiver_outgoing): (Sender<String>, Receiver<String>) = bounded(10);
      
        // RECEIVE INCOMING MESSAGES FROM TCP STREAM
        task::spawn(async move {
            loop {
                let mut buf = vec![0; 128]; 
                match reader.read_exact(&mut buf).await {
                    Ok(n) => {
                        let message = String::from_utf8_lossy(&buf[..n]).to_string();
                        println!("RECEIVED INCOMING MESSAGE: {}", message);
                        sender_incoming.send(message).await.unwrap();
                        buf.clear();
                    },
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
            loop{
                let received = receiver_outgoing.recv().await;
                match received {
                    Ok(msg) => {
                        match (writer).write_all(msg.as_bytes()).await {
                            Ok(_) => (),
                            Err(_) => {
                                let _ = writer.shutdown().await;
                            }
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
}