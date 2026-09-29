use binary_sv2::{B064KOwned, Deserialize, Serialize, U256Owned, B064K, U256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMsg {
    pub data: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroCopyMsg<'decoder> {
    pub channel_id: u32,
    pub merkle_root: U256<'decoder>,
    pub coinbase_suffix: B064K<'decoder>,
}

impl ZeroCopyMsgOwned {
    pub fn new_owned(channel_id: u32, coinbase_size: usize) -> Self {
        let merkle_root = U256Owned::try_from(vec![0x42u8; 32]).expect("U256 is exactly 32 bytes");
        let coinbase_suffix =
            B064KOwned::try_from(vec![0xABu8; coinbase_size]).expect("coinbase_size <= 65535");
        Self {
            channel_id,
            merkle_root,
            coinbase_suffix,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OwnedMsg {
    pub channel_id: u32,
    pub merkle_root: [u8; 32],
    pub coinbase_suffix: Vec<u8>,
}

impl OwnedMsg {
    #[allow(dead_code)]
    pub fn from_zc(msg: ZeroCopyMsg<'_>) -> Self {
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(msg.merkle_root.as_bytes());
        OwnedMsg {
            channel_id: msg.channel_id,
            merkle_root,
            coinbase_suffix: msg.coinbase_suffix.to_owned_bytes(),
        }
    }
}

// Each bench binary includes this file, so items only some of them use are dead in the others.
#[cfg(feature = "noise_sv2")]
#[allow(dead_code)]
mod noise {
    use codec_sv2::{Handshake, TransportDecryptState, TransportEncryptState};
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
    use noise_sv2::{
        Initiator, Responder, ELLSWIFT_ENCODING_SIZE, INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE,
    };

    const AUTHORITY_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
    const AUTHORITY_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
    const CERT_VALIDITY: core::time::Duration = core::time::Duration::from_secs(3600);

    pub fn make_handshake_pair() -> (Handshake<Initiator>, Handshake<Responder>) {
        let public_k: Secp256k1PublicKey = AUTHORITY_PUBLIC_K.to_string().try_into().unwrap();
        let private_k: Secp256k1SecretKey = AUTHORITY_PRIVATE_K.to_string().try_into().unwrap();

        let initiator = Initiator::from_raw_k(public_k.into_bytes()).unwrap();
        let responder = Responder::from_authority_kp(
            &public_k.into_bytes(),
            &private_k.into_bytes(),
            CERT_VALIDITY,
        )
        .unwrap();

        (
            Handshake::initiator(initiator),
            Handshake::responder(responder),
        )
    }

    pub fn make_transport_state_pair() -> (TransportEncryptState, TransportDecryptState) {
        let (initiator, responder) = make_handshake_pair();

        let (msg0, initiator) = initiator.step_0().unwrap();
        let msg0: [u8; ELLSWIFT_ENCODING_SIZE] = msg0.payload().try_into().unwrap();

        let (msg1, responder_transport) = responder.step_1(msg0).unwrap();
        let msg1: [u8; INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE] =
            msg1.payload().try_into().unwrap();

        let initiator_transport = initiator.step_2(msg1).unwrap();

        let (initiator_enc, _) = initiator_transport.split();
        let (_, responder_dec) = responder_transport.split();
        (initiator_enc, responder_dec)
    }
}

#[cfg(feature = "noise_sv2")]
#[allow(unused_imports)]
pub use noise::{make_handshake_pair, make_transport_state_pair};

#[cfg(feature = "with_buffer_pool")]
pub type Slice = buffer_sv2::Slice;

#[cfg(not(feature = "with_buffer_pool"))]
pub type Slice = Vec<u8>;

/// One encoded frame carrying a `ZeroCopyMsgOwned` with a coinbase of the given size.
#[allow(dead_code)]
pub fn make_encoded_frame(coinbase_size: usize) -> Vec<u8> {
    use framing_sv2::framing::{EncodableFrame, MessageFrame};

    let msg = ZeroCopyMsgOwned::new_owned(1, coinbase_size);
    let frame = MessageFrame::<ZeroCopyMsgOwned>::from_message(msg, 0, 0, true).unwrap();
    let mut buf = vec![0u8; frame.encoded_length()];
    frame.encode_into(&mut buf).unwrap();
    buf
}

/// Feeds `enc_buf` into `dec` a read window at a time until it yields a frame.
#[allow(dead_code)]
pub fn acquire_frame(
    dec: &mut codec_sv2::Decoder,
    enc_buf: &[u8],
) -> framing_sv2::framing::SerializedFrame<Slice> {
    let w = dec.read_buf();
    let header_len = w.len();
    w.copy_from_slice(&enc_buf[..header_len]);
    dec.advance(header_len).unwrap();
    let mut offset = header_len;
    loop {
        match dec.next_frame() {
            Ok(codec_sv2::Decoded::Frame(frame)) => return frame,
            Ok(codec_sv2::Decoded::Incomplete(_)) => {
                let w = dec.read_buf();
                let n = w.len();
                w.copy_from_slice(&enc_buf[offset..offset + n]);
                dec.advance(n).unwrap();
                offset += n;
            }
            Err(e) => panic!("decode error: {:?}", e),
        }
    }
}
