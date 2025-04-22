use crate::{
    error::Error,
    messages::{Message, Ping, Pong, PING_MSG_TYPE, PONG_MSG_TYPE},
};
use codec_sv2::{Frame, StandardEitherFrame, StandardSv2Frame};

use codec_sv2::{HandshakeRole, Responder};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection::Connection;

use async_channel::{Receiver, Sender};
use tokio::net::TcpListener;

pub async fn start_server(
    address: &str,
    k_pub: String,
    k_priv: String,
    cert_validity: std::time::Duration,
) -> Result<(), Error> {
    let listener = TcpListener::bind(address).await?;

    // parse keys
    let k_pub: Secp256k1PublicKey = k_pub.to_string().try_into()?;
    let k_priv: Secp256k1SecretKey = k_priv.to_string().try_into()?;

    println!("SERVER: Listening on {}", address);

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            // noise handshake responder
            let responder = Responder::from_authority_kp(
                &k_pub.into_bytes(),
                &k_priv.into_bytes(),
                cert_validity,
            )?;

            // channels for encrypted connection
            let (receiver, sender) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await?;

            // handle encrypted connection
            handle_connection(receiver, sender).await?;
            Ok::<(), Error>(())
        });
    }
}

async fn handle_connection(
    receiver: Receiver<StandardEitherFrame<Message>>,
    sender: Sender<StandardEitherFrame<Message>>,
) -> Result<(), Error> {
    // first, we need to read the ping frame
    // receiver already took care of decryption
    let mut frame: StandardSv2Frame<Message> = match receiver.recv().await {
        Ok(f) => f.try_into()?,
        Err(_) => return Err(Error::Receiver),
    };

    let frame_header = frame.get_header().ok_or(Error::FrameHeader)?;

    // check message type on header
    if frame_header.msg_type() != PING_MSG_TYPE {
        return Err(Error::WrongMessage);
    }

    // decode frame payload
    let decoded_payload: Ping = match binary_sv2::from_bytes(frame.payload()) {
        Ok(ping) => ping,
        Err(e) => return Err(Error::BinarySv2(e)),
    };

    // ok, we have successfully received the ping message
    // now it's time to send the pong response

    // we need the ping nonce to create our pong response
    let ping_nonce = decoded_payload.get_nonce();

    println!("SERVER: Received encrypted Ping with nonce: {}", ping_nonce);

    // create Pong message
    let pong = Pong::new(ping_nonce)?;
    let message = Message::Pong(pong.clone());

    // create Pong frame
    let pong_frame =
        StandardSv2Frame::<Message>::from_message(message.clone(), PONG_MSG_TYPE, 0, false)
            .ok_or(Error::FrameFromMessage)?;

    // respond Pong (sender takes care of encryption)
    println!(
        "SERVER: Sending encrypted Pong to client with nonce: {}",
        pong.get_nonce()
    );
    sender
        .send(pong_frame.into())
        .await
        .map_err(|_| Error::Sender)?;

    Ok(())
}
