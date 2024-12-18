// # Sv2 Frame Example
//
// This example demonstrates how to use the `framing_sv2` crate to construct, serialize, and
// deserialize a regular Sv2 message frame (`Sv2Frame`). It showcases how to:
//
// - Define a message payload and frame it using the Sv2 protocol.
// - Define a custom message type (`CustomMessage`) to be framed.
// - Construct an Sv2 frame with the custom message, message type, and extension type.
// - Serialize the frame into a byte array.
// - Deserialize the frame from the byte array back into an Sv2 frame.
//
// In the Sv2 protocol, these frames can then be encoded and transmitted between Sv2 roles.
//
// ## Run
//
// ```
// cargo run --example sv2_frame --features with_serde
// ```

#[cfg(feature = "with_serde")]
use binary_sv2::{GetSize, Serialize};
#[cfg(feature = "with_serde")]
use framing_sv2::framing::Sv2Frame;

// Example message type (e.g., SetupConnection)
#[cfg(feature = "with_serde")]
const MSG_TYPE: u8 = 1;
// Example extension type (e.g., a standard Sv2 message)
#[cfg(feature = "with_serde")]
const EXT_TYPE: u16 = 0x0001;

#[cfg(feature = "with_serde")]
#[derive(Serialize, Debug)]
struct CustomMessage {
    pub data: u32,
}

// Implemented to help determine the size of the message when framing.
#[cfg(feature = "with_serde")]
impl GetSize for CustomMessage {
    fn get_size(&self) -> usize {
        4 // `data` is `u32`, which is 4 bytes
    }
}

#[cfg(feature = "with_serde")]
fn main() {
    // Create the message payload
    let message = CustomMessage { data: 42 };

    // Create the frame from the message
    let frame: Sv2Frame<CustomMessage, Vec<u8>> =
        Sv2Frame::from_message(message, MSG_TYPE, EXT_TYPE, false)
            .expect("Failed to frame the message");

    // How header information is accessed
    let header = frame.get_header().expect("Frame has no header");
    assert_eq!(header.msg_type(), MSG_TYPE);
    assert_eq!(header.ext_type(), EXT_TYPE);

    // Serialize the frame into a byte array for transmission
    let mut serialized_frame = vec![0u8; frame.encoded_length()];
    frame
        .serialize(&mut serialized_frame)
        .expect("Failed to serialize the frame");

    // Deserialize the frame from bytes back into an Sv2Frame
    let mut deserialized_frame = Sv2Frame::<CustomMessage, Vec<u8>>::from_bytes(serialized_frame)
        .expect("Failed to deserialize frame");
    assert_eq!(deserialized_frame.encoded_length(), 10); // 6 header bytes + 4 payload bytes
    assert_eq!(deserialized_frame.payload(), [42, 0, 0, 0]);
}

#[cfg(not(feature = "with_serde"))]
fn main() {
    eprintln!("Serde feature not enabled. Skipping example.");
}
