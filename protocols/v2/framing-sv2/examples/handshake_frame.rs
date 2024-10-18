// # Noise Handshake Frame Example
//
// This example demonstrates how to use the `framing_sv2` crate to handle handshake frames
// (`HandShakeFrame`) used during the Noise protocol handshake process. It showcases how to:
//
// - Define a handshake payload to be framed.
// - Construct a handshake frame with the provided payload.
// - Serialize the handshake frame into a byte array.
// - Deserialize the handshake frame from the byte array back into a `HandShakeFrame`.
// - Verify that the handshake frame is correctly processed.
//
// In the Sv2 protocol, these frames are specialized for Noise protocol handshakes required to
// establish a secure communication channel between Sv2 roles.
//
// ## Run
//
// ```
// cargo run --example handshake_frame
// ```

use framing_sv2::framing::HandShakeFrame;

fn main() {
    // Define an example handshake message payload (Noise protocol handshake)
    let handshake_message = vec![0x01, 0x02, 0x03, 0x04];

    // Frame the message using `HandShakeFrame`
    let handshake_frame = HandShakeFrame::from_bytes(handshake_message.into())
        .expect("Failed to create handshake frame");

    // Serialize the handshake frame into a byte array for transmission
    let serialized_handshake = handshake_frame.get_payload_when_handshaking();
    println!("Serialized Handshake Frame: {:?}", serialized_handshake);

    // Deserialize the frame back from the byte array into a `HandShakeFrame`
    let deserialized_frame = HandShakeFrame::from_bytes(serialized_handshake.into())
        .expect("Failed to deserialize handshake frame");

    // Verify that the deserialized frame matches the original
    assert_eq!(
        deserialized_frame.get_payload_when_handshaking(),
        vec![0x01, 0x02, 0x03, 0x04]
    );
}
