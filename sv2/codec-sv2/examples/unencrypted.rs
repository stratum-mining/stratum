// # Using Sv2 Codec Without Encryption
//
// This example demonstrates how to use the `codec-sv2` crate to encode and decode Sv2 frames
// without encryption over a TCP connection. It showcases how to:
//
// * Create an arbitrary custom message type (`CustomMessage`) for encoding and decoding.
// * Encode the message into a Sv2 frame.
// * Send the encoded frame over a TCP connection.
// * Decode the Sv2 frame on the receiving side and extract the original message.
//
// ## Run
//
// ```
// cargo run --example unencrypted
// ```

use binary_sv2::{binary_codec_sv2, Deserialize, Serialize};
use codec_sv2::{Encoder, Error, StandardDecoder, StandardSv2Frame, Sv2Frame};
use std::{
    convert::TryInto,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

// The `with_buffer_pool` feature changes some type signatures.
#[cfg(not(feature = "with_buffer_pool"))]
type Slice = Vec<u8>;

#[cfg(feature = "with_buffer_pool")]
type Slice = buffer_sv2::Slice;

// Arbitrary message type.
// Supported Sv2 message types are listed in the [Sv2 Spec Message
// Types](https://github.com/stratum-mining/sv2-spec/blob/main/08-Message-Types.md).
const CUSTOM_MSG_TYPE: u8 = 0xff;

// Emulate a TCP connection
const TCP_ADDR: &str = "127.0.0.1:3333";

// Example message type.
// In practice, all Sv2 messages are defined in the following crates:
// * `common_messages_sv2`
// * `mining_sv2`
// * `job_declaration_sv2`
// * `template_distribution_sv2`
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomMessage {
    nonce: u16,
}

fn main() {
    // Start a receiving listener
    let listener_receiver = TcpListener::bind(TCP_ADDR).expect("Failed to bind TCP listener");

    // Start a sender stream
    let stream_sender: TcpStream =
        TcpStream::connect(TCP_ADDR).expect("Failed to connect to TCP stream");

    // Start a receiver stream
    let stream_receiver: TcpStream = listener_receiver
        .incoming()
        .next()
        .expect("Failed to accept incoming TCP stream")
        .expect("Failed to connect to incoming TCP stream");

    // Server Side

    // Create a message
    let nonce = 1337;
    let msg = CustomMessage { nonce };
    let msg_type = CUSTOM_MSG_TYPE;
    // Unique identifier of the extension describing the protocol message, as defined by Sv2 Framing
    let extension_type = 0;
    // This message is intended for the receiver, so set to false
    let channel_msg = false;
    sender_side(stream_sender, msg, msg_type, extension_type, channel_msg);

    // Receiver Side
    let mut decoded_frame = receiver_side(stream_receiver);

    // Parse the decoded frame header and payload
    let decoded_frame_header = decoded_frame
        .get_header()
        .expect("Failed to get the frame header");
    let decoded_msg: CustomMessage = binary_sv2::from_bytes(decoded_frame.payload())
        .expect("Failed to extract the message from the payload");

    // Assert that the decoded message is as expected
    assert_eq!(decoded_frame_header.msg_type(), CUSTOM_MSG_TYPE);
    assert_eq!(decoded_msg.nonce, nonce);
}

fn sender_side(
    mut stream_sender: TcpStream,
    msg: CustomMessage,
    msg_type: u8,
    extension_type: u16,
    channel_msg: bool,
) {
    // Create the frame
    let frame =
        StandardSv2Frame::<CustomMessage>::from_message(msg, msg_type, extension_type, channel_msg)
            .expect("Failed to create the frame");

    // Encode the frame
    let mut encoder = Encoder::<CustomMessage>::new();
    let encoded_frame = encoder
        .encode(frame.clone())
        .expect("Failed to encode the frame");

    // Send the encoded frame
    stream_sender
        .write_all(encoded_frame)
        .expect("Failed to send the encoded frame");
}

fn receiver_side(mut stream_receiver: TcpStream) -> Sv2Frame<CustomMessage, Slice> {
    // Initialize the decoder
    let mut decoder = StandardDecoder::<CustomMessage>::new();

    // Continuously read the frame from the TCP stream into the decoder buffer until the full
    // message is received.
    //
    // Note: The length of the payload is defined in a header field. Every call to `next_frame`
    // will return a `MissingBytes` error, until the full payload is received.
    loop {
        let decoder_buf = decoder.writable();

        // Read the frame header into the decoder buffer
        stream_receiver
            .read_exact(decoder_buf)
            .expect("Failed to read the encoded frame header");

        match decoder.next_frame() {
            Ok(decoded_frame) => {
                return decoded_frame;
            }
            Err(Error::MissingBytes(_)) => {}
            Err(_) => panic!("Failed to decode the frame"),
        }
    }
}
