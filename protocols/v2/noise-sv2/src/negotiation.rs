use crate::Error;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Seq0255, Serialize};
use bytes::{Buf, BufMut};
use core::convert::{TryFrom, TryInto};
use snow::{params::NoiseParams, Builder};

/// Builds noise params given a certain EncryptionAlgorithm
pub struct NoiseParamsBuilder {
    params: NoiseParams,
}

impl NoiseParamsBuilder {
    pub fn new(chosen_algorithm: EncryptionAlgorithm) -> Self {
        let pattern = match chosen_algorithm {
            EncryptionAlgorithm::AesGcm => "Noise_NX_25519_AESGCM_BLAKE2s",
            EncryptionAlgorithm::ChaChaPoly => "Noise_NX_25519_ChaChaPoly_BLAKE2s",
        };
        Self {
            params: pattern.parse().expect("BUG: cannot parse noise parameters"),
        }
    }

    pub fn get_builder<'a>(self) -> Builder<'a> {
        // Initialize our initiator using a builder.
        Builder::new(self.params)
    }
}

/// Negotiation prologue; if initiator and responder prologue don't match the entire negotiation
/// fails.
/// Made of the initiator message (the list of algorithms) and the responder message (the
/// algorithm chosen). If both of them are None, no negotiation happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prologue<'a> {
    pub possible_algos: &'a [EncryptionAlgorithm],
    pub chosen_algo: EncryptionAlgorithm,
}

impl<'d> Prologue<'d> {
    pub fn serialize_to_buf(&self, buf: &mut Vec<u8>) {
        let algo_list_len = self.possible_algos.len() as u8;
        buf.put_u8(algo_list_len);
        for algo in self.possible_algos.iter() {
            algo.serialize_to_buf(buf);
        }
        self.chosen_algo.serialize_to_buf(buf)
    }
}

const MAGIC: u32 = u32::from_le_bytes(*b"STR3");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    AesGcm,
    ChaChaPoly,
}

impl EncryptionAlgorithm {
    const AESGCM: u32 = u32::from_le_bytes(*b"AESG");
    const CHACHAPOLY: u32 = u32::from_le_bytes(*b"CHCH");

    pub fn serialize_to_buf(&self, buf: &mut Vec<u8>) {
        buf.put_u32_le(u32::from(*self));
    }

    pub fn deserialize_from_buf(buf: &[u8]) -> Result<Self, Error> {
        if buf.remaining() < std::mem::size_of::<u32>() {
            return Err(Error::InvalidHandshakeMessage);
        }
        let mut raw_repr = [0_u8; 4];
        raw_repr.copy_from_slice(&buf[0..4]);
        Self::try_from(u32::from_le_bytes(raw_repr))
    }
}

impl From<EncryptionAlgorithm> for u32 {
    fn from(value: EncryptionAlgorithm) -> Self {
        match value {
            EncryptionAlgorithm::AesGcm => EncryptionAlgorithm::AESGCM,
            EncryptionAlgorithm::ChaChaPoly => EncryptionAlgorithm::CHACHAPOLY,
        }
    }
}
impl TryFrom<u32> for EncryptionAlgorithm {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            Self::AESGCM => Ok(EncryptionAlgorithm::AesGcm),
            Self::CHACHAPOLY => Ok(EncryptionAlgorithm::ChaChaPoly),
            _ => Err(Error::EncryptionAlgorithmInvalid(value)),
        }
    }
}

/// Message used for negotiation of the encryption algorithm
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NegotiationMessage<'decoder> {
    magic: u32,
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    encryption_algos: Seq0255<'decoder, u32>,
}

impl<'decoder> NegotiationMessage<'decoder> {
    pub fn new(encryption_algos: Vec<EncryptionAlgorithm>) -> Self {
        let algos: Vec<u32> = encryption_algos.into_iter().map(|x| x.into()).collect();
        Self {
            magic: MAGIC,
            encryption_algos: algos.try_into().unwrap(),
        }
    }

    pub fn get_algos(&self) -> Result<Vec<EncryptionAlgorithm>, crate::Error> {
        let mut algos = vec![];
        #[cfg(not(feature = "with_serde"))]
        let algos_: Vec<u32> = self.encryption_algos.0.clone();
        #[cfg(feature = "with_serde")]
        let algos_: Vec<u32> = self.encryption_algos.clone().into();
        for algo in algos_ {
            let algo: EncryptionAlgorithm = algo.try_into()?;
            algos.push(algo);
        }
        Ok(algos)
    }
}
