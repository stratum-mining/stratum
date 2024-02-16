use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};
use std::{convert::TryInto, time::SystemTime};

pub struct SignatureNoiseMessage {
    pub version: u16,
    pub valid_from: u32,
    pub not_valid_after: u32,
    pub signature: [u8; 64],
}

impl From<[u8; 74]> for SignatureNoiseMessage {
    fn from(value: [u8; 74]) -> Self {
        let version = u16::from_le_bytes(value[0..2].try_into().unwrap());
        let valid_from = u32::from_le_bytes(value[2..6].try_into().unwrap());
        let not_valid_after = u32::from_le_bytes(value[6..10].try_into().unwrap());
        let signature = value[10..74].try_into().unwrap();
        Self {
            version,
            valid_from,
            not_valid_after,
            signature,
        }
    }
}

impl SignatureNoiseMessage {
    pub fn verify(self, pk: &XOnlyPublicKey, authority_pk: &Option<XOnlyPublicKey>) -> bool {
        if let Some(authority_pk) = authority_pk {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32;
            if self.valid_from <= now && self.not_valid_after >= now {
                let secp = Secp256k1::verification_only();
                let (m, s) = self.split();
                // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
                let m = [&m[0..10], &pk.serialize()].concat();
                let m = Message::from_hashed_data::<sha256::Hash>(&m);
                let s = match Signature::from_slice(&s) {
                    Ok(s) => s,
                    _ => return false,
                };
                secp.verify_schnorr(&s, &m, authority_pk).is_ok()
            } else {
                false
            }
        } else {
            true
        }
    }
    pub fn sign(msg: &mut [u8; 74], static_pk: &XOnlyPublicKey, kp: &Keypair) {
        let secp = Secp256k1::signing_only();
        let m = [&msg[0..10], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        let signature = secp.sign_schnorr(&m, kp);
        for (i, b) in signature.as_ref().iter().enumerate() {
            msg[10 + i] = *b;
        }
    }

    fn split(self) -> ([u8; 10], [u8; 64]) {
        let mut m = [0; 10];
        m[0] = self.version.to_le_bytes()[0];
        m[1] = self.version.to_le_bytes()[1];
        m[2] = self.valid_from.to_le_bytes()[0];
        m[3] = self.valid_from.to_le_bytes()[1];
        m[4] = self.valid_from.to_le_bytes()[2];
        m[5] = self.valid_from.to_le_bytes()[3];
        m[6] = self.not_valid_after.to_le_bytes()[0];
        m[7] = self.not_valid_after.to_le_bytes()[1];
        m[8] = self.not_valid_after.to_le_bytes()[2];
        m[9] = self.not_valid_after.to_le_bytes()[3];
        (m, self.signature)
    }
}
