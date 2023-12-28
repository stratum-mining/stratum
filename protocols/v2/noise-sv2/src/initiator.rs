use std::{convert::TryInto, ptr};

use crate::{
    cipher_state::{Cipher, CipherState, GenericCipher},
    error::Error,
    handshake::HandshakeOp,
    signature_message::SignatureNoiseMessage,
    NoiseCodec,
};
use aes_gcm::KeyInit;
use chacha20poly1305::ChaCha20Poly1305;
use secp256k1::{KeyPair, XOnlyPublicKey};

pub struct Initiator {
    handshake_cipher: Option<ChaCha20Poly1305>,
    k: Option<[u8; 32]>,
    n: u64,
    // Chaining key
    ck: [u8; 32],
    // Handshake hash
    h: [u8; 32],
    // ephemeral keypair
    e: KeyPair,
    // upstream pub key
    #[allow(unused)]
    pk: XOnlyPublicKey,
    c1: Option<GenericCipher>,
    c2: Option<GenericCipher>,
}

impl std::fmt::Debug for Initiator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Initiator").finish()
    }
}

// Make sure that Initiator is not sync so we do not need to worry about what other memory accessor see
// after that we zeroize k is send cause if we send it the original thread can not access
// anymore it
//impl<C: AeadCipher> !Sync for Initiator<C> {}
//impl<C: AeadCipher> !Copy for Initiator<C> {}

impl CipherState<ChaCha20Poly1305> for Initiator {
    fn get_k(&mut self) -> &mut Option<[u8; 32]> {
        &mut self.k
    }
    fn get_n(&self) -> u64 {
        self.n
    }
    fn set_n(&mut self, n: u64) {
        self.n = n;
    }
    fn get_cipher(&mut self) -> &mut Option<ChaCha20Poly1305> {
        &mut self.handshake_cipher
    }

    fn set_k(&mut self, k: Option<[u8; 32]>) {
        self.k = k;
    }
}

impl HandshakeOp<ChaCha20Poly1305> for Initiator {
    fn name(&self) -> String {
        "Initiator".to_string()
    }
    fn get_h(&mut self) -> &mut [u8; 32] {
        &mut self.h
    }

    fn get_ck(&mut self) -> &mut [u8; 32] {
        &mut self.ck
    }

    fn set_h(&mut self, data: [u8; 32]) {
        self.h = data;
    }

    fn set_ck(&mut self, data: [u8; 32]) {
        self.ck = data;
    }

    fn set_handshake_cipher(&mut self, cipher: ChaCha20Poly1305) {
        self.handshake_cipher = Some(cipher);
    }
}

impl Initiator {
    pub fn from_raw_k(key: [u8; 32]) -> Result<Box<Self>, Error> {
        let pk =
            secp256k1::XOnlyPublicKey::from_slice(&key).map_err(|_| Error::InvalidRawPublicKey)?;
        Ok(Self::new(pk))
    }

    pub fn new(pk: XOnlyPublicKey) -> Box<Self> {
        let mut self_ = Self {
            handshake_cipher: None,
            k: None,
            n: 0,
            ck: [0; 32],
            h: [0; 32],
            e: Self::generate_key(),
            pk,
            c1: None,
            c2: None,
        };
        self_.initialize_self();
        Box::new(self_)
    }

    /// #### 4.5.1.1 Initiator
    ///
    /// Initiator generates ephemeral keypair and sends the public key to the responder:
    ///
    /// 1. initializes empty output buffer
    /// 2. generates ephemeral keypair `e`, appends `e.public_key` to the buffer (32 bytes plaintext public key)
    /// 3. calls `MixHash(e.public_key)`
    /// 4. calls `EncryptAndHash()` with empty payload and appends the ciphertext to the buffer (note that _k_ is empty at this point, so this effectively reduces down to `MixHash()` on empty data)
    /// 5. submits the buffer for sending to the responder in the following format
    ///
    /// ##### Ephemeral public key message:
    ///
    /// | Field name | Description                      |
    /// | ---------- | -------------------------------- |
    /// | PUBKEY     | Initiator's ephemeral public key |
    ///
    /// Message length: 32 bytes
    pub fn step_0(&mut self) -> Result<[u8; 32], aes_gcm::Error> {
        let serialized = self.e.public_key().x_only_public_key().0.serialize();
        self.mix_hash(&serialized);
        self.encrypt_and_hash(&mut vec![])?;

        let mut message = [0u8; 32];
        message[..32].copy_from_slice(&serialized[..32]);
        Ok(message)
    }

    /// #### 4.5.2.2 Initiator
    ///
    /// 1. receives NX-handshake part 2 message
    /// 2. interprets first 32 bytes as `re.public_key`
    /// 3. calls `MixHash(re.public_key)`
    /// 4. calls `MixKey(ECDH(e.private_key, re.public_key))`
    /// 5. decrypts next 48 bytes with `DecryptAndHash()` and stores the results as `rs.public_key` which is **server's static public key** (note that 32 bytes is the public key and 16 bytes is MAC)
    /// 6. calls `MixKey(ECDH(e.private_key, rs.public_key)`
    /// 7. decrypts next 90 bytes with `DecryptAndHash()` and deserialize plaintext into `SIGNATURE_NOISE_MESSAGE` (74 bytes data + 16 bytes MAC)
    /// 8. return pair of CipherState objects, the first for encrypting transport messages from initiator to responder, and the second for messages in the other direction:
    ///    1. sets `temp_k1, temp_k2 = HKDF(ck, zerolen, 2)`
    ///    2. creates two new CipherState objects `c1` and `c2`
    ///    3. calls `c1.InitializeKey(temp_k1)` and `c2.InitializeKey(temp_k2)`
    ///    4. returns the pair `(c1, c2)`
    ///
    ///
    ///
    pub fn step_2(&mut self, message: [u8; 170]) -> Result<NoiseCodec, Error> {
        // 2. interprets first 32 bytes as `re.public_key`
        // 3. calls `MixHash(re.public_key)`
        let remote_pub_key = &message[0..32];
        self.mix_hash(remote_pub_key);

        // 4. calls `MixKey(ECDH(e.private_key, re.public_key))`
        let e_private_key = self.e.secret_bytes();
        self.mix_key(&Self::ecdh(&e_private_key[..], remote_pub_key)[..]);

        // 5. decrypts next 48 bytes with `DecryptAndHash()` and stores the results as `rs.public_key` which is **server's static public key** (note that 32 bytes is the public key and 16 bytes is MAC)
        let mut to_decrypt = message[32..80].to_vec();

        self.decrypt_and_hash(&mut to_decrypt)?;
        let rs_pub_key = to_decrypt;

        // 6. calls `MixKey(ECDH(e.private_key, rs.public_key)`
        self.mix_key(&Self::ecdh(&e_private_key[..], &rs_pub_key[..])[..]);

        let mut to_decrypt = message[80..170].to_vec();
        self.decrypt_and_hash(&mut to_decrypt)?;
        let plaintext: [u8; 74] = to_decrypt.try_into().unwrap();
        let signature_message: SignatureNoiseMessage = plaintext.into();
        let rs_pk_xonly = XOnlyPublicKey::from_slice(&rs_pub_key).unwrap();
        if signature_message.verify(&rs_pk_xonly) {
            let (temp_k1, temp_k2) = Self::hkdf_2(self.get_ck(), &[]);
            let c1 = ChaCha20Poly1305::new(&temp_k1.into());
            let c2 = ChaCha20Poly1305::new(&temp_k2.into());
            let c1: Cipher<ChaCha20Poly1305> = Cipher::from_key_and_cipher(temp_k1, c1);
            let c2: Cipher<ChaCha20Poly1305> = Cipher::from_key_and_cipher(temp_k2, c2);
            self.c1 = None;
            self.c2 = None;
            let mut encryptor = GenericCipher::ChaCha20Poly1305(c1);
            let mut decryptor = GenericCipher::ChaCha20Poly1305(c2);
            encryptor.erase_k();
            decryptor.erase_k();
            let codec = crate::NoiseCodec {
                encryptor,
                decryptor,
            };
            Ok(codec)
        } else {
            Err(Error::InvalidCertificate(plaintext))
        }
    }

    fn erase(&mut self) {
        if let Some(k) = self.k.as_mut() {
            for b in k {
                unsafe { ptr::write_volatile(b, 0) };
            }
        }
        for mut b in self.ck {
            unsafe { ptr::write_volatile(&mut b, 0) };
        }
        for mut b in self.h {
            unsafe { ptr::write_volatile(&mut b, 0) };
        }
        if let Some(c1) = self.c1.as_mut() {
            c1.erase_k()
        }
        if let Some(c2) = self.c2.as_mut() {
            c2.erase_k()
        }
        self.e.non_secure_erase();
    }
}
impl Drop for Initiator {
    fn drop(&mut self) {
        self.erase();
    }
}
