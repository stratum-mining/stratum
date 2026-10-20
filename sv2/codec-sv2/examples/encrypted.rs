// # Using Sv2 Codec with Noise Encryption
//
// This example demonstrates how to use the `codec-sv2` crate to encode and decode Sv2 frames
// with Noise protocol encryption over a TCP connection. It showcases how to:
//
// * Perform a Noise handshake between the sender and receiver.
// * Create an arbitrary custom message type (`CustomMessage`) for encryption/encoding and
//   decryption/decoding.
// * Encode the message into an encrypted Sv2 frame using Noise.
// * Send the encrypted frame over a TCP connection.
// * Decode the encrypted Sv2 frame on the receiving side after completing the Noise handshake.
//
// ## Run
//
// ```
// cargo run --example encrypted --features noise_sv2
// ```

use binary_sv2::{Deserialize, Serialize};
#[cfg(feature = "noise_sv2")]
use codec_sv2::{Decrypted, Handshake, MessageFrame, NoiseDecoder, NoiseEncoder};
#[cfg(feature = "noise_sv2")]
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
#[cfg(feature = "noise_sv2")]
use noise_sv2::{Initiator, Responder};

#[cfg(feature = "noise_sv2")]
use noise_sv2::{ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE};

use std::convert::TryInto;
#[cfg(feature = "noise_sv2")]
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

// Arbitrary message type.
// Supported Sv2 message types are listed in the [Sv2 Spec Message
// Types](https://github.com/stratum-mining/sv2-spec/blob/main/08-Message-Types.md).
#[cfg(feature = "noise_sv2")]
const CUSTOM_MSG_TYPE: u8 = 0xff;

// Emulate a TCP connection
#[cfg(feature = "noise_sv2")]
const TCP_ADDR: &str = "127.0.0.1:3333";

#[cfg(feature = "noise_sv2")]
const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
#[cfg(feature = "noise_sv2")]
const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
#[cfg(feature = "noise_sv2")]
const CERT_VALIDITY: std::time::Duration = std::time::Duration::from_secs(3600);

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

#[cfg(feature = "noise_sv2")]
fn main() {
    // Start a receiving listener
    let listener_receiver = TcpListener::bind(TCP_ADDR).expect("Failed to bind TCP listener");

    // Start a sender stream
    let mut stream_sender: TcpStream =
        TcpStream::connect(TCP_ADDR).expect("Failed to connect to TCP stream");

    // Start a receiver stream
    let mut stream_receiver: TcpStream = listener_receiver
        .incoming()
        .next()
        .expect("Failed to accept incoming TCP stream")
        .expect("Failed to connect to incoming TCP stream");

    // Handshake
    let authority_public_k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K
        .to_string()
        .try_into()
        .expect("Failed to convert receiver public key to Secp256k1PublicKey");

    let authority_private_k: Secp256k1SecretKey = AUTHORITY_PRIVATE_K
        .to_string()
        .try_into()
        .expect("Failed to convert receiver private key to Secp256k1PublicKey");

    #[cfg(feature = "std")]
    let initiator = Initiator::from_raw_k(authority_public_k.into_bytes())
        .expect("Failed to create initiator role from raw pub key");
    #[cfg(not(feature = "std"))]
    let initiator =
        Initiator::from_raw_k_with_rng(authority_public_k.into_bytes(), &mut rand::thread_rng())
            .expect("Failed to create initiator role from raw pub key");

    #[cfg(feature = "std")]
    let responder = Responder::from_authority_kp(
        &authority_public_k.into_bytes(),
        &authority_private_k.into_bytes(),
        CERT_VALIDITY,
    )
    .expect("Failed to initialize responder from pub/key pair and/or cert");
    #[cfg(not(feature = "std"))]
    let responder = Responder::from_authority_kp_with_rng(
        &authority_public_k.into_bytes(),
        &authority_private_k.into_bytes(),
        CERT_VALIDITY,
        &mut rand::thread_rng(),
    )
    .expect("Failed to initialize responder from pub/key pair and/or cert");

    let sender = Handshake::initiator(initiator);
    let receiver = Handshake::responder(responder);

    let (first_message, sender) = sender
        .step_0()
        .expect("Initiator failed first step of handshake");
    let first_message: [u8; ELLSWIFT_ENCODING_SIZE] = first_message
        .payload()
        .try_into()
        .expect("Handshake remote invlaid message");

    #[cfg(feature = "std")]
    let (second_message, receiver_transport) = receiver
        .step_1(first_message)
        .expect("Responder failed second step of handshake");
    #[cfg(not(feature = "std"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    #[cfg(not(feature = "std"))]
    let (second_message, receiver_transport) = receiver
        .step_1_with_now_rng(first_message, now, &mut rand::thread_rng())
        .expect("Responder failed second step of handshake");
    let second_message: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] = second_message
        .payload()
        .try_into()
        .expect("Handshake remote invlaid message");

    #[cfg(feature = "std")]
    let sender_transport = sender
        .step_2(second_message)
        .expect("Initiator failed third step of handshake");
    #[cfg(not(feature = "std"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    #[cfg(not(feature = "std"))]
    let sender_transport = sender
        .step_2_with_now(second_message, now)
        .expect("Initiator failed third step of handshake");

    // The handshake is over: each side now holds only the half it needs.
    let (mut sender_encrypt, _) = sender_transport.split();
    let (_, mut receiver_decrypt) = receiver_transport.split();

    // Create a message
    let nonce = 1337;
    let msg = CustomMessage { nonce };
    let msg_type = CUSTOM_MSG_TYPE;
    // Unique identifier of the extension describing the protocol message, as defined by Sv2 Framing
    let extension_type = 0;
    // This message is intended for the receiver, so set to false
    let channel_msg = false;

    let frame: MessageFrame<CustomMessage> =
        MessageFrame::from_message(msg, msg_type, extension_type, channel_msg)
            .expect("Failed to create the frame");

    let mut encoder = NoiseEncoder::new();
    let encoded_frame = encoder
        .encode_transport(frame, &mut sender_encrypt)
        .expect("Failed to encode the frame");

    // Send the encoded frame
    stream_sender
        .write_all(&encoded_frame[..])
        .expect("Failed to send the encoded frame");

    // Initialize the decoder
    let mut decoder = NoiseDecoder::new();

    // Continuously read the frame from the TCP stream into the decoder buffer until the full
    // message is received.
    //
    // Note: The length of the payload is defined in a header field. Every call to
    // `next_transport_frame` returns `Incomplete`, until the full payload is received.
    let mut decoded_frame = loop {
        // Read whatever the stream has into the decoder's window, then report how much arrived
        let read = stream_receiver
            .read(decoder.read_buf())
            .expect("Failed to read the encoded frame");
        assert_ne!(read, 0, "The stream closed before the frame was complete");
        decoder
            .advance(read)
            .expect("Read more than the window holds");

        match decoder.next_transport_frame(receiver_decrypt) {
            Ok(Decrypted::Frame(frame, _)) => break frame,
            Ok(Decrypted::Incomplete(_, state)) => receiver_decrypt = state,
            Err(_) => panic!("Failed to decode the frame"),
        }
    };

    // Parse the decoded frame header and payload
    let decoded_frame_header = decoded_frame.header();

    let decoded_msg: CustomMessage = binary_sv2::from_bytes(decoded_frame.payload())
        .expect("Failed to extract the message from the payload");

    // Assert that the decoded message is as expected
    assert_eq!(decoded_frame_header.msg_type(), CUSTOM_MSG_TYPE);
    assert_eq!(decoded_msg.nonce, nonce);
}

#[cfg(not(feature = "noise_sv2"))]
fn main() {
    eprintln!("Noise feature not enabled. Skipping example.");
}
