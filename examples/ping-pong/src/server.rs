use crate::{
    error::Error,
    messages::{Ping, Pong, PING_MSG_TYPE, PONG_MSG_TYPE},
};
use codec_sv2::{Frame, StandardDecoder, StandardSv2Frame};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use codec_sv2::framing_sv2::header::Header as StandardSv2Header;

pub fn start_server(address: &str) -> Result<(), Error> {
    let listener = TcpListener::bind(address)?;

    println!("SERVER: Listening on {}", address);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| {
                    handle_connection(stream)?;
                    Ok::<(), Error>(())
                });
            }
            Err(e) => return Err(Error::Tcp(e)),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> Result<(), Error> {
    // first, we need to read the ping message

    // initialize decoder
    let mut decoder = StandardDecoder::<Ping>::new();

    // right now, the decoder buffer can only read a frame header
    // because decoder.missing_b is initialized with a header size
    let decoder_buf = decoder.writable();

    // read frame header into decoder_buf
    stream.read_exact(decoder_buf)?;

    // this returns an error (MissingBytes), because it only read the header, and there's no payload
    // in memory yet therefore, we safely ignore the error
    // the important thing here is that we loaded decoder.missing_b with the expected frame payload
    // size
    let _ = decoder.next_frame();

    // now, the decoder buffer has the expected size of the frame payload
    let decoder_buf = decoder.writable();

    // read from stream into decoder_buf again, loading the payload into memory
    stream.read_exact(decoder_buf)?;

    // parse into a Sv2Frame
    let mut frame: StandardSv2Frame<Ping> = decoder.next_frame()?;
    let frame_header: StandardSv2Header = frame.get_header().ok_or(Error::FrameHeader)?;

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

    println!("SERVER: Received Ping message with nonce: {}", ping_nonce);

    // create Pong message
    let pong_message = Pong::new(ping_nonce)?;

    // create Pong frame
    let pong_frame =
        StandardSv2Frame::<Pong>::from_message(pong_message.clone(), PONG_MSG_TYPE, 0, false)
            .ok_or(Error::FrameFromMessage)?;

    // encode Pong frame
    let mut encoder = codec_sv2::Encoder::<Pong>::new();
    let pong_encoded = encoder.encode(pong_frame)?;

    println!(
        "SERVER: Sending Pong to client with nonce: {}",
        pong_message.get_nonce()
    );
    stream.write_all(pong_encoded)?;

    Ok(())
}
