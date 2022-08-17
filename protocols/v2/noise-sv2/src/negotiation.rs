use binary_sv2::{binary_codec_sv2, Deserialize, Seq0255, Serialize};
use core::convert::{TryFrom, TryInto};
use snow::{params::NoiseParams, Builder};

/// Builds noise params given a certain EncryptionAlgorithm
pub struct NoiseParamsBuilder {
    params: NoiseParams,
}

impl NoiseParamsBuilder {
    pub fn new(chosen_algorithm: EncryptionAlgorithm) -> Self {
        Self {
            params: format!("Noise_NX_25519_{:?}_BLAKE2s", chosen_algorithm)
                .parse()
                .expect("BUG: cannot parse noise parameters"),
        }
    }

    pub fn get_builder<'a>(self) -> Builder<'a> {
        // Initialize our initiator using a builder.
        Builder::new(self.params)
    }
}

const MAGIC: u32 = u32::from_le_bytes(*b"STR2");

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum EncryptionAlgorithm {
    AESGCM,
    ChaChaPoly,
}

impl From<EncryptionAlgorithm> for u32 {
    fn from(value: EncryptionAlgorithm) -> Self {
        match value {
            EncryptionAlgorithm::AESGCM => u32::from_le_bytes(*b"AESG"),
            EncryptionAlgorithm::ChaChaPoly => u32::from_le_bytes(*b"CHCH"),
        }
    }
}
impl TryFrom<u32> for EncryptionAlgorithm {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            //32::from_le_bytes(*b"AESG");
            1196639553 => Ok(EncryptionAlgorithm::AESGCM),
            //32::from_le_bytes(*b"CHCH");
            1212368963 => Ok(EncryptionAlgorithm::ChaChaPoly),
            _ => Err(()),
        }
    }
}

/// Message used for negotiation of the encryption algorithm
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NegotiationMessage<'decoder> {
    magic: u32,
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
        for algo in &self.encryption_algos.0 {
            let algo: EncryptionAlgorithm =
                (*algo).try_into().map_err(|_| crate::Error::NoiseTodo)?;
            algos.push(algo);
        }
        Ok(algos)
    }
}
