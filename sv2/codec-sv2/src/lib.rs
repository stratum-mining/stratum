//! # Stratum V2 Codec Library
//!
//! `codec_sv2` provides the message encoding and decoding functionality for the Stratum V2 (Sv2)
//! protocol, handling secure communication between Sv2 roles.
//!
//! This crate abstracts the complexity of message encoding/decoding with optional Noise protocol
//! support, ensuring both regular and encrypted messages can be serialized, transmitted, and
//! decoded consistently and reliably.
//!
//!
//! ## Usage
//! Encode with the [`Encoder`], or the `NoiseEncoder` under the `noise_sv2` feature.
//!
//! Decode with the [`Decoder`], or the `NoiseDecoder` under the same feature.
//!
//! The `state` module gives each phase of a Noise connection its own type.
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//!
//! - `std`: Enable usage of rust `std` library, enabled by default.
//! - `noise_sv2`: Enables support for Noise protocol encryption and decryption.
//! - `with_buffer_pool`: Enables buffer pooling for more efficient memory management.
//!
//! In order to use this crate in a `#![no_std]` environment, use the `--no-default-features` to
//! remove the `std` feature.
//!
//! ## Examples
//!
//! See the examples for more information:
//!
//! - [Unencrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/unencrypted.rs)
//! - [Encrypted Example](https://github.com/stratum-mining/stratum/blob/main/protocols/v2/codec-sv2/examples/encrypted.rs)

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use buffer_sv2::Buffer as IsBuffer;
#[cfg(feature = "noise_sv2")]
use framing_sv2::{header::Header, SV2_FRAME_CHUNK_SIZE, SV2_FRAME_HEADER_SIZE};
#[cfg(feature = "noise_sv2")]
use noise_sv2::AEAD_MAC_LEN;

pub mod decoder;
pub mod encoder;
pub mod error;
#[cfg(feature = "noise_sv2")]
pub mod state;
#[cfg(all(test, feature = "noise_sv2"))]
pub(crate) mod test_utils;

pub use error::{Error, Result};

#[cfg(feature = "noise_sv2")]
pub use framing_sv2::framing::HandshakeMessage;
pub use framing_sv2::framing::{EncodableFrame, MessageFrame, SizeHint};

pub use decoder::{Decoded, Decoder};
#[cfg(feature = "noise_sv2")]
pub use decoder::{Decrypted, NoiseDecoder};

pub use encoder::Encoder;
#[cfg(feature = "noise_sv2")]
pub use encoder::NoiseEncoder;

#[cfg(feature = "noise_sv2")]
pub use state::{
    ExpectsHandshakeMessage, Handshake, InitiatorSent, Transport, TransportDecryptState,
    TransportEncryptState,
};

#[cfg(not(feature = "with_buffer_pool"))]
pub(crate) type Buffer = buffer_sv2::BufferFromSystemMemory;

#[cfg(feature = "with_buffer_pool")]
pub(crate) type Buffer = buffer_sv2::BufferPool<buffer_sv2::BufferFromSystemMemory>;

/// Size of an encrypted Sv2 frame header, including the MAC that seals it.
#[cfg(feature = "noise_sv2")]
pub const ENCRYPTED_SV2_FRAME_HEADER_SIZE: usize = SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN;

/// Plaintext a single chunk carries: a whole chunk less the MAC that seals it.
#[cfg(feature = "noise_sv2")]
pub const SV2_FRAME_PLAINTEXT_CHUNK_SIZE: usize = SV2_FRAME_CHUNK_SIZE - AEAD_MAC_LEN;

/// Length the payload `header` declares takes once encrypted, including the MAC of every chunk.
#[cfg(feature = "noise_sv2")]
pub fn encrypted_payload_length(header: &Header) -> usize {
    let len = header.payload_length();
    let chunks = len.div_ceil(SV2_FRAME_PLAINTEXT_CHUNK_SIZE);
    len + chunks * AEAD_MAC_LEN
}

pub(crate) const DEFAULT_POOL_BUFFER_SIZE: usize = 2_usize.pow(16) * 5;

/// A [`framing_sv2::framing::SerializedFrame`] over the buffer the `with_buffer_pool` feature
/// selects, which is what the decoders hand back and what an encoder takes to forward one on.
///
/// With `with_buffer_pool`, a decoded frame holds a slot of the decoder's pool until it is
/// dropped: decode its message and drop it, or keep a copy made with
/// [`into_owned`](framing_sv2::framing::SerializedFrame::into_owned).
pub type SerializedFrame = framing_sv2::framing::SerializedFrame<<Buffer as IsBuffer>::Slice>;

#[cfg(test)]
#[cfg(feature = "noise_sv2")]
mod tests {
    use crate::{
        test_utils::{decode_noise_frame, make_handshake_pair, transport_halves},
        Decoded, InitiatorSent, NoiseDecoder, NoiseEncoder, TransportDecryptState,
        TransportEncryptState, SV2_FRAME_PLAINTEXT_CHUNK_SIZE,
    };
    use binary_sv2::{Deserialize, Serialize, B064K};
    use framing_sv2::{framing::MessageFrame, header::Header, SV2_FRAME_HEADER_SIZE};
    use noise_sv2::{
        Responder, AEAD_MAC_LEN, ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE,
    };
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    const MSG_TYPE: u8 = 0xff;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestMsg {
        nonce: u16,
    }

    // A message whose payload can be made large enough to span more than one chunk.
    #[derive(Debug, Serialize, Deserialize)]
    struct ChunkedMsg<'decoder> {
        data: B064K<'decoder>,
    }

    fn round_trip(
        encoder: &mut NoiseEncoder,
        decoder: &mut NoiseDecoder,
        enc: &mut TransportEncryptState,
        dec: TransportDecryptState,
        nonce: u16,
    ) -> (u16, TransportDecryptState) {
        let frame = MessageFrame::from_message(TestMsg { nonce }, MSG_TYPE, 0, false).unwrap();
        let encrypted = encoder.encode_transport(frame, enc).unwrap();

        let (mut frame, dec) = decode_noise_frame(decoder, dec, encrypted.as_ref())
            .expect("failed to decode a transport frame");
        assert_eq!(frame.header().msg_type(), MSG_TYPE);
        let msg: TestMsg = binary_sv2::from_bytes(frame.payload()).unwrap();
        (msg.nonce, dec)
    }

    #[test]
    fn split_transport_round_trips_in_both_directions() {
        let (mut initiator_enc, mut initiator_dec, mut responder_enc, mut responder_dec) =
            transport_halves();
        let mut encoder = NoiseEncoder::new();

        let mut to_responder = NoiseDecoder::new();
        let mut to_initiator = NoiseDecoder::new();

        for nonce in 0..8u16 {
            let (decoded, dec) = round_trip(
                &mut encoder,
                &mut to_responder,
                &mut initiator_enc,
                responder_dec,
                nonce,
            );
            assert_eq!(decoded, nonce);
            responder_dec = dec;

            let (decoded, dec) = round_trip(
                &mut encoder,
                &mut to_initiator,
                &mut responder_enc,
                initiator_dec,
                nonce + 100,
            );
            assert_eq!(decoded, nonce + 100);
            initiator_dec = dec;
        }
    }

    #[test]
    fn split_transport_round_trips_a_frame_that_spans_several_chunks() {
        let (mut initiator_enc, _, _, responder_dec) = transport_halves();

        let mut data = vec![0xab; u16::MAX as usize];
        assert!(data.len() > SV2_FRAME_PLAINTEXT_CHUNK_SIZE);
        let msg = ChunkedMsg {
            data: (&mut data[..]).try_into().unwrap(),
        };

        let frame = MessageFrame::from_message(msg, MSG_TYPE, 0, false).unwrap();
        let mut encoder = NoiseEncoder::new();
        let encrypted = encoder.encode_transport(frame, &mut initiator_enc).unwrap();

        let mut decoder = NoiseDecoder::new();
        let (mut frame, _) = decode_noise_frame(&mut decoder, responder_dec, encrypted.as_ref())
            .expect("failed to decode a chunked transport frame");
        assert_eq!(frame.header().msg_type(), MSG_TYPE);
        let decoded: ChunkedMsg = binary_sv2::from_bytes(frame.payload()).unwrap();
        assert_eq!(decoded.data.as_bytes(), &vec![0xab; u16::MAX as usize][..]);
    }

    /// The handshake runs through the codec itself rather than around it.
    #[test]
    fn a_handshake_round_trips_through_the_codec() {
        let (initiator, responder) = make_handshake_pair();

        let mut encoder = NoiseEncoder::new();
        let mut to_responder = NoiseDecoder::new();
        let mut to_initiator = NoiseDecoder::new();

        let (first, initiator) = initiator.step_0().unwrap();
        let first = encoder.encode_handshake(first);
        let first: &[u8] = first.as_ref();
        assert_eq!(first.len(), ELLSWIFT_ENCODING_SIZE);
        let first = read_handshake_frame::<Responder>(&mut to_responder, first);
        let first: [u8; ELLSWIFT_ENCODING_SIZE] = first.payload().try_into().unwrap();

        let (second, responder_transport) = responder.step_1(first).unwrap();
        let second = encoder.encode_handshake(second);
        let second: &[u8] = second.as_ref();
        assert_eq!(second.len(), INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE);
        let second = read_handshake_frame::<InitiatorSent>(&mut to_initiator, second);
        let second: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
            second.payload().try_into().unwrap();

        let initiator_transport = initiator.step_2(second).unwrap();

        let (mut initiator_enc, _) = initiator_transport.split();
        let (_, responder_dec) = responder_transport.split();
        let (decoded, _) = round_trip(
            &mut encoder,
            &mut NoiseDecoder::new(),
            &mut initiator_enc,
            responder_dec,
            7,
        );
        assert_eq!(decoded, 7);
    }

    fn read_handshake_frame<R: crate::ExpectsHandshakeMessage>(
        decoder: &mut NoiseDecoder,
        encoded: &[u8],
    ) -> framing_sv2::framing::HandshakeMessage {
        let mut offset = 0;
        loop {
            let writable = decoder.read_buf();
            let len = writable.len();
            writable.copy_from_slice(&encoded[offset..offset + len]);
            decoder.advance(len).unwrap();
            offset += len;

            match decoder.next_handshake_frame::<R>() {
                Ok(Decoded::Frame(frame)) => return frame,
                Ok(Decoded::Incomplete(_)) => {}
                Err(e) => panic!("failed to decode a handshake frame: {e:?}"),
            }
        }
    }

    #[derive(Debug, Clone)]
    struct ValidU24(u32);

    impl Arbitrary for ValidU24 {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            ValidU24(u32::arbitrary(g) % 16_777_216)
        }
    }

    /// The encrypted length grows by one MAC per chunk, not one MAC overall.
    #[quickcheck]
    fn prop_encrypted_payload_length_counts_one_mac_per_chunk(payload_length: ValidU24) {
        let mut bytes = [0u8; SV2_FRAME_HEADER_SIZE];
        bytes[2] = 0x01;
        bytes[3..].copy_from_slice(&payload_length.0.to_le_bytes()[..3]);
        let header = Header::from_bytes(&bytes).unwrap();

        let payload_per_chunk = SV2_FRAME_PLAINTEXT_CHUNK_SIZE;
        let chunks = (payload_length.0 as usize).div_ceil(payload_per_chunk);

        assert_eq!(
            crate::encrypted_payload_length(&header),
            payload_length.0 as usize + chunks * AEAD_MAC_LEN,
            "mismatch for a {}-byte payload spanning {chunks} chunks",
            payload_length.0
        );
    }
}
