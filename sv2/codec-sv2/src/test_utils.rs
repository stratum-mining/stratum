//! Handshake and decode helpers the crate's own tests share.
//!
//! Building a transport pair takes the same twenty lines every time, so it lived in three test
//! modules at once; the authority keys below were spelled out in six places.

use crate::{
    decoder::{Decrypted, NoiseDecoder},
    state::Handshake,
    Buffer, SerializedFrame, TransportDecryptState, TransportEncryptState,
};
use buffer_sv2::Buffer as IsBuffer;
use core::time::Duration;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use noise_sv2::{
    Initiator, Responder, ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE,
};

pub(crate) const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
pub(crate) const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
pub(crate) const CERT_VALIDITY: Duration = Duration::from_secs(3600);

pub(crate) type Slice = <Buffer as IsBuffer>::Slice;

/// The two roles, each in its starting handshake state.
pub(crate) fn make_handshake_pair() -> (Handshake<Initiator>, Handshake<Responder>) {
    let pub_k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
    let pub_k_bytes = pub_k.into_bytes();
    let priv_k: Secp256k1SecretKey = AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();
    let priv_k_bytes = priv_k.into_bytes();

    (
        Handshake::initiator(Initiator::from_raw_k(pub_k_bytes).unwrap()),
        Handshake::responder(
            Responder::from_authority_kp(&pub_k_bytes, &priv_k_bytes, CERT_VALIDITY).unwrap(),
        ),
    )
}

/// Runs a handshake to completion and returns all four halves it produces, initiator first.
pub(crate) fn transport_halves() -> (
    TransportEncryptState,
    TransportDecryptState,
    TransportEncryptState,
    TransportDecryptState,
) {
    let (initiator, responder) = make_handshake_pair();

    let (msg0, initiator) = initiator.step_0().unwrap();
    let msg0: [u8; ELLSWIFT_ENCODING_SIZE] = msg0.payload().try_into().unwrap();

    let (msg1, responder_transport) = responder.step_1(msg0).unwrap();
    let msg1: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] = msg1.payload().try_into().unwrap();

    let initiator_transport = initiator.step_2(msg1).unwrap();

    let (initiator_enc, initiator_dec) = initiator_transport.split();
    let (responder_enc, responder_dec) = responder_transport.split();
    (initiator_enc, initiator_dec, responder_enc, responder_dec)
}

/// The initiator's encrypting half and the responder's decrypting half of one handshake.
pub(crate) fn make_transport_state_pair() -> (TransportEncryptState, TransportDecryptState) {
    let (initiator_enc, _, _, responder_dec) = transport_halves();
    (initiator_enc, responder_dec)
}

/// Feeds `encoded` into `decoder` a read window at a time until it yields a frame.
pub(crate) fn decode_noise_frame(
    decoder: &mut NoiseDecoder,
    mut state: TransportDecryptState,
    encoded: &[u8],
) -> Option<(SerializedFrame, TransportDecryptState)> {
    let mut offset = 0;
    loop {
        let writable = decoder.read_buf();
        let available = encoded.len().saturating_sub(offset);
        let n = writable.len().min(available);
        writable[..n].copy_from_slice(&encoded[offset..offset + n]);
        decoder.advance(n).unwrap();
        offset += n;

        match decoder.next_transport_frame(state) {
            Ok(Decrypted::Frame(frame, state)) => return Some((frame, state)),
            Ok(Decrypted::Incomplete(_, next)) => state = next,
            Err(_) => return None,
        }
    }
}
