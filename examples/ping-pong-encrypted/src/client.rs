use crate::messages::{Message, Ping, Pong, PING_MSG_TYPE, PONG_MSG_TYPE};
use codec_sv2::{Frame, HandshakeRole, Initiator, StandardSv2Frame};
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection::Connection;
use tokio::net::TcpStream;

use crate::error::Error;

pub async fn start_client(address: &str, k_pub: String) -> Result<(), Error> {
    let stream = TcpStream::connect(address).await?;

    println!("CLIENT: Connected to server on {}", address);

    // parse server pubkey
    let k_pub: Secp256k1PublicKey = k_pub.try_into()?;

    // noise handshake initiator
    let initiator = Initiator::from_raw_k(k_pub.into_bytes())?;

    // channels for encrypted connection
    let (receiver, sender) = Connection::new(stream, HandshakeRole::Initiator(initiator)).await?;

    // create Ping message
    let ping = Ping::new()?;
    let ping_nonce = ping.get_nonce();
    let message = Message::Ping(ping);

    // create Ping frame
    let ping_frame =
        StandardSv2Frame::<Message>::from_message(message.clone(), PING_MSG_TYPE, 0, false)
            .ok_or(Error::FrameFromMessage)?;

    // send Ping frame (sender takes care of encryption)
    println!(
        "CLIENT: Sending encrypted Ping to server with nonce: {}",
        ping_nonce
    );
    sender
        .send(ping_frame.into())
        .await
        .map_err(|_| Error::Sender)?;

    // ok, we have successfully sent the ping message
    // now it's time to receive and verify the pong response
    // receiver already took care of decryption
    let mut frame: StandardSv2Frame<Message> = match receiver.recv().await {
        Ok(f) => f.try_into()?,
        Err(_) => return Err(Error::Receiver),
    };

    let frame_header = frame.get_header().ok_or(Error::FrameHeader)?;

    // check message type on header
    if frame_header.msg_type() != PONG_MSG_TYPE {
        return Err(Error::FrameHeader);
    }

    // decode frame payload
    let decoded_payload: Pong = match binary_sv2::from_bytes(frame.payload()) {
        Ok(pong) => pong,
        Err(e) => return Err(Error::BinarySv2(e)),
    };

    // check if nonce is the same as ping
    let pong_nonce = decoded_payload.get_nonce();
    if ping_nonce == pong_nonce {
        println!(
            "CLIENT: Received encrypted Pong with identical nonce as Ping: {}",
            pong_nonce
        );
    } else {
        return Err(Error::Nonce);
    }

    Ok(())
}
