// # Noise Handshake Operations
//
// The [`HandshakeOp`] trait defines the cryptographic operations and utilities required to perform
// the Noise protocol handshake between Sv2 roles.
//
// This trait abstracts key management, encryption, and hashing for the Noise protocol handshake,
// outlining core operations implemented by the [`crate::Initiator`] and [`crate::Responder`]
// roles. The trait governs the following processes:
//
// - Elliptic curve Diffie-Hellman (ECDH) key exchange using the [`secp256k1`] curve to establish a
//   shared secret.
// - HMAC and HKDF for deriving encryption keys from the shared secret.
// - AEAD encryption and decryption using either [`ChaCha20Poly1305`] or `AES-GCM` ciphers to ensure
//   message confidentiality and integrity.
// - Chaining key and handshake hash updates to maintain the security of the session.
//
// The handshake begins with the exchange of ephemeral key pairs, followed by the derivation of
// shared secrets, which are then used to securely encrypt all subsequent communication.
//
// ## Usage
// The handshake secures communication between two Sv2 roles, with one acting as the
// [`crate::Initiator`] (e.g., a local mining proxy) and the other as the [`crate::Responder`]
// (e.g., a remote pool). Both roles implement the [`HandshakeOp`] trait to manage cryptographic
// state, updating the handshake hash (`h`), chaining key (`ck`), and encryption key (`k`) to
// ensure confidentiality and integrity throughout the handshake.
//
// Securing communication via the Noise protocol guarantees the confidentiality and authenticity of
// sensitive data, such as share submissions. While the use of a secure channel is optional for Sv2
// roles within a local network (e.g., between a local mining device and mining proxy), it is
// mandatory for communication across external networks (e.g., between a local mining proxy and a
// remote pool).

use alloc::{string::String, vec::Vec};

use crate::{aed_cipher::AeadCipher, cipher_state::CipherState, NOISE_HASHED_PROTOCOL_NAME_CHACHA};
use chacha20poly1305::ChaCha20Poly1305;
use secp256k1::{
    ecdh::SharedSecret,
    hashes::{sha256::Hash as Sha256Hash, Hash},
    rand, Keypair, Secp256k1, SecretKey, XOnlyPublicKey,
};

// Represents the operations needed during a Noise protocol handshake.
//
// The [`HandshakeOp`] trait defines the necessary functions for managing the state and
// cryptographic operations required during the Noise protocol handshake. It provides methods for
// key generation, hash mixing, encryption, decryption, and key derivation, ensuring that the
// handshake process is secure and consistent.
pub trait HandshakeOp<Cipher: AeadCipher>: CipherState<Cipher> {
    // Returns the name of the entity implementing the handshake operation.
    //
    // Provides a string that identifies the entity (e.g., "Initiator" or "Responder") that is
    // performing the handshake. It is primarily used for debugging or logging purposes.
    #[allow(dead_code)]
    fn name(&self) -> String;

    // Retrieves a mutable reference to the handshake hash (`h`).
    //
    // The handshake hash accumulates the state of the handshake, incorporating all exchanged
    // messages to ensure integrity and prevent tampering. This method provides access to the
    // current state of the handshake hash, allowing it to be updated as the handshake progresses.
    fn get_h(&mut self) -> &mut [u8; 32];

    // Retrieves a mutable reference to the chaining key (`ck`).
    //
    // The chaining key is used during the key derivation process to generate new keys throughout
    // the handshake. This method provides access to the current chaining key, which is updated
    // as the handshake progresses and new keys are derived.
    fn get_ck(&mut self) -> &mut [u8; 32];

    // Sets the handshake hash (`h`) to the provided value.
    //
    // This method allows the handshake hash to be explicitly set, typically after it has been
    // initialized or updated during the handshake process. The handshake hash ensures the
    // integrity of the handshake by incorporating all exchanged messages.
    fn set_h(&mut self, data: [u8; 32]);

    // Sets the chaining key (`ck`) to the provided value.
    //
    // This method allows the chaining key to be explicitly set, typically after it has been
    // initialized or updated during the handshake process. The chaining key is crucial for
    // deriving new keys as the handshake progresses.
    fn set_ck(&mut self, data: [u8; 32]);

    // Mixes the data into the handshake hash (`h`).
    //
    // Updates the current handshake hash by combining it with the provided `data`. The result is
    // a new SHA-256 hash digest that reflects all previous handshake messages, ensuring the
    // integrity of the handshake process. This method is typically called whenever a new piece of
    // data (e.g., a public key or ciphertext) needs to be incorporated into the handshake state.
    fn mix_hash(&mut self, data: &[u8]) {
        let h = self.get_h();
        let mut to_hash = Vec::with_capacity(32 + data.len());
        to_hash.extend_from_slice(h);
        to_hash.extend_from_slice(data);
        *h = Sha256Hash::hash(&to_hash).to_byte_array();
    }

    // Generates a new cryptographic key pair using the [`Secp256k1`] curve.
    //
    // Generates a fresh key pair, consisting of a secret key and a corresponding public key,
    // using the [`Secp256k1`] elliptic curve. If the generated public key does not match the
    // expected parity, a new key pair is generated to ensure consistency.
    #[allow(dead_code)]
    #[cfg(feature = "std")]
    fn generate_key() -> Keypair {
        Self::generate_key_with_rng(&mut rand::thread_rng())
    }
    #[inline]
    fn generate_key_with_rng<R: rand::Rng + ?Sized>(rng: &mut R) -> Keypair {
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(rng);
        let kp = Keypair::from_secret_key(&secp, &secret_key);
        if kp.x_only_public_key().1 == crate::PARITY {
            kp
        } else {
            Self::generate_key_with_rng(rng)
        }
    }

    // Computes an HMAC-SHA256 (Hash-based Message Authentication Code) hash of the provided data
    // using the given key.
    //
    // This method implements the HMAC-SHA256 hashing algorithm, which combines a key and data to
    // produce a 32-byte hash. It is used during the handshake to securely derive new keys from
    // existing material, ensuring that the resulting keys are cryptographically strong.
    //
    // This method uses a two-step process:
    // 1. The key is XORed with an inner padding (`ipad`) and hashed with the data.
    // 2. The result is XORed with the outer padding (`opad`) and hashed again to produce the final
    //    HMAC.
    fn hmac_hash(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
        #[allow(clippy::identity_op)]
        let mut ipad = [(0 ^ 0x36); 64];
        #[allow(clippy::identity_op)]
        let mut opad = [(0 ^ 0x5c); 64];
        for i in 0..32 {
            ipad[i] = key[i] ^ 0x36;
        }
        for i in 0..32 {
            opad[i] = key[i] ^ 0x5c;
        }

        let mut to_hash = Vec::with_capacity(64 + data.len());
        to_hash.extend_from_slice(&ipad);
        to_hash.extend_from_slice(data);
        let temp = Sha256Hash::hash(&to_hash).to_byte_array();

        to_hash.clear();
        to_hash.extend_from_slice(&opad);
        to_hash.extend_from_slice(&temp);

        Sha256Hash::hash(&to_hash).to_byte_array()
    }

    // Derives two new keys using the HKDF (HMAC-based Key Derivation Function) process.
    //
    // Performs the HKDF key derivation process, which uses an initial chaining key and input key
    // material to produce two new 32-byte keys. This process is used throughout the handshake to
    // generate fresh keys for encryption and authentication, ensuring that each step of the
    // handshake is securely linked.
    //
    // This method performs the following steps:
    // 1. Performs a HMAC hash on the chaining key and input key material to derive a temporary key.
    // 2. Performs a HMAC hash on the temporary key and specific byte sequence (`0x01`) to derive
    //    the first output.
    // 3. Performs a HMAC hash on the temporary key and the concatenation of the first output and a
    //    specific byte sequence (`0x02`).
    // 4. Returns both outputs.
    fn hkdf_2(chaining_key: &[u8; 32], input_key_material: &[u8]) -> ([u8; 32], [u8; 32]) {
        let temp_key = Self::hmac_hash(chaining_key, input_key_material);
        let out_1 = Self::hmac_hash(&temp_key, &[0x1]);
        let out_2 = Self::hmac_hash(&temp_key, &[&out_1[..], &[0x2][..]].concat());
        (out_1, out_2)
    }

    #[allow(dead_code)]
    fn hkdf_3(
        chaining_key: &[u8; 32],
        input_key_material: &[u8],
    ) -> ([u8; 32], [u8; 32], [u8; 32]) {
        let temp_key = Self::hmac_hash(chaining_key, input_key_material);
        let out_1 = Self::hmac_hash(&temp_key, &[0x1]);
        let out_2 = Self::hmac_hash(&temp_key, &[&out_1[..], &[0x2][..]].concat());
        let out_3 = Self::hmac_hash(&temp_key, &[&out_2[..], &[0x3][..]].concat());
        (out_1, out_2, out_3)
    }

    // Mixes the input key material into the current chaining key (`ck`) and initializes the
    // handshake cipher with an updated encryption key (`k`).
    //
    // Updates the chaining key by incorporating the provided input key material (e.g., the result
    // of a Diffie-Hellman exchange) and uses the updated chaining key to derive a new encryption
    // key. The encryption key is then used to initialize the handshake cipher, preparing it for
    // use in the next step of the handshake.
    fn mix_key(&mut self, input_key_material: &[u8]) {
        let ck = self.get_ck();
        let (ck, temp_k) = Self::hkdf_2(ck, input_key_material);
        self.set_ck(ck);
        self.initialize_key(temp_k);
    }

    // Encrypts the provided plaintext and updates the hash `h` value.
    //
    // The `encrypt_and_hash` method encrypts the given plaintext using the
    // current encryption key and then updates `h` with the resulting ciphertext.
    // If an encryption key is present `k`, the method encrypts the data using
    // using AEAD, where the associated data is the current hash value. After
    // encryption, the ciphertext is mixed into the hash to ensure integrity
    // and authenticity of the messages exchanged during the handshake.
    fn encrypt_and_hash(&mut self, plaintext: &mut Vec<u8>) -> Result<(), aes_gcm::Error> {
        if self.get_k().is_some() {
            #[allow(clippy::clone_on_copy)]
            let h = self.get_h().clone();
            self.encrypt_with_ad(&h, plaintext)?;
        };
        let ciphertext = plaintext;
        self.mix_hash(ciphertext);
        Ok(())
    }

    // Decrypts the provided ciphertext and updates the handshake hash (`h`).
    //
    // Decrypts the given ciphertext using the handshake cipher and then mixes the ciphertext
    // (before decryption) into the handshake hash. If the encryption key (`k`) is present, the
    // data is decrypted using AEAD, where the associated data is the current handshake hash. This
    // ensures that each decryption step is securely linked to the previous handshake state,
    // maintaining the integrity of the
    // handshake.
    fn decrypt_and_hash(&mut self, ciphertext: &mut Vec<u8>) -> Result<(), aes_gcm::Error> {
        let encrypted = ciphertext.clone();
        if self.get_k().is_some() {
            #[allow(clippy::clone_on_copy)]
            let h = self.get_h().clone();
            self.decrypt_with_ad(&h, ciphertext)?;
        };
        self.mix_hash(&encrypted);
        Ok(())
    }

    #[allow(dead_code)]
    fn ecdh(private: &[u8], public: &[u8]) -> [u8; 32] {
        let private = SecretKey::from_slice(private).expect("Wrong key");
        let x_public = XOnlyPublicKey::from_slice(public).expect("Wrong key");
        let res = SharedSecret::new(&x_public.public_key(crate::PARITY), &private);
        res.secret_bytes()
    }

    // Initializes the handshake state by setting the initial chaining key (`ck`) and handshake
    // hash (`h`).
    //
    // Prepares the handshake state for use by setting the initial chaining key and handshake
    // hash. The chaining key is typically derived from a protocol name or other agreed-upon
    // value, and the handshake hash is initialized to reflect this starting state.
    fn initialize_self(&mut self) {
        let ck = NOISE_HASHED_PROTOCOL_NAME_CHACHA;
        let h = Sha256Hash::hash(&ck[..]);
        self.set_h(h.to_byte_array());
        self.set_ck(ck);
        self.set_k(None);
    }

    // Initializes the handshake cipher with the provided encryption key (`k`).
    //
    // Resets the nonce (`n`) to 0 and initializes the handshake cipher using the given 32-byte
    // encryption key. It also updates the internal key storage (`k`) with the new key, preparing
    // the cipher for encrypting or decrypting subsequent messages in the handshake.
    fn initialize_key(&mut self, key: [u8; 32]) {
        self.set_n(0);
        let cipher = ChaCha20Poly1305::from_key(key);
        self.set_handshake_cipher(cipher);
        if let Some(k) = self.get_k() {
            *k = key;
        } else {
            let set_k = self.get_k();
            *set_k = Some(key);
        }
    }

    fn set_handshake_cipher(&mut self, cipher: ChaCha20Poly1305);
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::string::ToString;
    use core::convert::TryInto;
    use quickcheck::{Arbitrary, TestResult};

    use secp256k1::SecretKey;

    struct TestHandShake {
        k: Option<[u8; 32]>,
        n: u64,
        cipher: Option<ChaCha20Poly1305>,
        h: [u8; 32],
        ck: [u8; 32],
    }

    impl TestHandShake {
        pub fn new() -> Self {
            let mut self_ = TestHandShake {
                k: None,
                n: 0,
                cipher: None,
                h: [0; 32],
                ck: [0; 32],
            };
            self_.initialize_self();
            self_
        }
    }

    impl CipherState<ChaCha20Poly1305> for TestHandShake {
        fn get_k(&mut self) -> &mut Option<[u8; 32]> {
            &mut self.k
        }

        fn set_k(&mut self, k: Option<[u8; 32]>) {
            self.k = k
        }

        fn get_n(&self) -> u64 {
            self.n
        }

        fn set_n(&mut self, n: u64) {
            self.n = n
        }

        fn get_cipher(&mut self) -> &mut Option<ChaCha20Poly1305> {
            &mut self.cipher
        }
    }

    impl HandshakeOp<ChaCha20Poly1305> for TestHandShake {
        fn name(&self) -> String {
            "Test".to_string()
        }

        fn get_h(&mut self) -> &mut [u8; 32] {
            &mut self.h
        }

        fn get_ck(&mut self) -> &mut [u8; 32] {
            &mut self.ck
        }

        fn set_h(&mut self, data: [u8; 32]) {
            self.h = data
        }

        fn set_ck(&mut self, data: [u8; 32]) {
            self.ck = data
        }

        fn set_handshake_cipher(&mut self, cipher: ChaCha20Poly1305) {
            self.cipher = Some(cipher)
        }
    }

    #[test]
    fn is_a_cypher() {
        let mut cipher_1 = TestHandShake::new();
        let mut cipher_2 = TestHandShake::new();
        cipher_1.initialize_key([0; 32]);
        cipher_2.initialize_key([0; 32]);

        let ad = [1, 2, 3];
        let data = vec![1, 7, 92, 3, 4, 5];

        let mut encrypted = data.clone();
        cipher_1.encrypt_with_ad(&ad, &mut encrypted).unwrap();

        cipher_2.decrypt_with_ad(&ad, &mut encrypted).unwrap();

        assert!(encrypted == data);
    }

    #[test]
    fn test_hmac_hash_with_0s() {
        let k = [0; 32];
        let data = [0; 90];
        let value = TestHandShake::hmac_hash(&k, &data);

        // xor padded key with repeted 0x36
        let xored = [0x36; 64];
        let mut to_hash = vec![];
        for b in xored {
            to_hash.push(b);
        }
        for b in data {
            to_hash.push(b);
        }
        let temp = Sha256Hash::hash(&to_hash).to_byte_array();
        // xor padded key with repeted 0x5x 01011100
        let xored = [0x5c; 64];
        let mut to_hash = vec![];
        for b in xored {
            to_hash.push(b);
        }
        for b in temp {
            to_hash.push(b);
        }
        let expected = Sha256Hash::hash(&to_hash).to_byte_array();

        assert!(value == expected);
    }

    #[test]
    fn test_hkdf2() {
        let chaining_key = [0; 32];
        let input_key_material = [0; 32];
        let temp_k = TestHandShake::hmac_hash(&chaining_key, &input_key_material);
        let expected_1 = TestHandShake::hmac_hash(&temp_k, &[0x1]);
        let mut temp_2 = expected_1.to_vec();
        temp_2.push(0x2);
        let expected_2 = TestHandShake::hmac_hash(&temp_k, &temp_2);
        let (out_1, out_2) = TestHandShake::hkdf_2(&chaining_key, &input_key_material);
        assert!(out_1 == expected_1);
        assert!(out_2 == expected_2);
    }

    #[test]
    fn test_mix_key() {
        let input_key_material = [0; 32];
        let ck = [0; 32];
        let mut tester = TestHandShake::new();
        tester.set_ck(ck);

        let (mut ck, temp_k) = TestHandShake::hkdf_2(&ck, &input_key_material);

        tester.mix_key(&input_key_material);

        assert!(tester.get_ck() == &mut ck);
        assert!(tester.get_k().unwrap() == temp_k);
    }

    #[test]
    fn test_mix_hash() {
        let data = [0; 32];
        let h = [0; 32];
        let mut tester = TestHandShake::new();
        tester.set_h(h);

        let mut to_hash = h.to_vec();
        to_hash.extend_from_slice(&data);
        let mut expected = Sha256Hash::hash(&to_hash).to_byte_array();

        tester.mix_hash(&data);

        assert!(tester.get_h() == &mut expected);
    }

    #[test]
    fn test_decrypt_encrypt_with_hash() {
        let mut cipher_1 = TestHandShake::new();
        let mut cipher_2 = TestHandShake::new();
        cipher_1.initialize_key([0; 32]);
        cipher_2.initialize_key([0; 32]);

        cipher_1.set_h([0; 32]);
        cipher_2.set_h([0; 32]);

        let data = vec![1, 7, 92, 3, 4, 5];

        let mut encrypted = data.clone();
        cipher_1.encrypt_and_hash(&mut encrypted).unwrap();
        assert!(encrypted != data);

        cipher_2.decrypt_and_hash(&mut encrypted).unwrap();

        assert!(encrypted == data);
        assert!(cipher_1.get_h() == cipher_2.get_h());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_ecdh() {
        let key_pair_1 = TestHandShake::generate_key();
        let key_pair_2 = TestHandShake::generate_key();

        let secret_1 = key_pair_1.secret_bytes();
        let secret_2 = key_pair_2.secret_bytes();

        let pub_1 = key_pair_1.x_only_public_key();
        let pub_2 = key_pair_2.x_only_public_key();

        let ecdh_1 = TestHandShake::ecdh(&secret_1, &pub_2.0.serialize());
        let ecdh_2 = TestHandShake::ecdh(&secret_2, &pub_1.0.serialize());

        assert!(ecdh_1 == ecdh_2);
    }

    #[test]
    fn test_ecdh_with_rng() {
        let key_pair_1 = TestHandShake::generate_key_with_rng(&mut rand::thread_rng());
        let key_pair_2 = TestHandShake::generate_key_with_rng(&mut rand::thread_rng());

        let secret_1 = key_pair_1.secret_bytes();
        let secret_2 = key_pair_2.secret_bytes();

        let pub_1 = key_pair_1.x_only_public_key();
        let pub_2 = key_pair_2.x_only_public_key();

        let ecdh_1 = TestHandShake::ecdh(&secret_1, &pub_2.0.serialize());
        let ecdh_2 = TestHandShake::ecdh(&secret_2, &pub_1.0.serialize());

        assert!(ecdh_1 == ecdh_2);
    }

    #[derive(Clone, Debug)]
    struct KeypairWrapper(pub Option<Keypair>);

    impl Arbitrary for KeypairWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let secp = Secp256k1::new();
            let mut secret = Vec::<u8>::arbitrary(g);
            if secret.len() < 32 {
                while secret.len() < 32 {
                    secret.push(0)
                }
            }
            if secret.len() > 32 {
                secret.truncate(32);
            }
            assert!(secret.len() == 32);
            let secret: [u8; 32] = secret.try_into().unwrap();
            match SecretKey::from_slice(&secret) {
                Ok(secret) => KeypairWrapper(Some(Keypair::from_secret_key(&secp, &secret))),
                Err(_) => KeypairWrapper(None),
            }
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_ecdh_1(kp1: KeypairWrapper, kp2: KeypairWrapper) -> TestResult {
        let (kp1, kp2) = match (kp1.0, kp2.0) {
            (Some(kp1), Some(kp2)) => (kp1, kp2),
            _ => return TestResult::discard(),
        };
        if kp1.x_only_public_key().1 == crate::PARITY && kp2.x_only_public_key().1 == crate::PARITY
        {
            let secret_1 = kp1.secret_bytes();
            let secret_2 = kp2.secret_bytes();

            let pub_1 = kp1.x_only_public_key();
            let pub_2 = kp2.x_only_public_key();

            let ecdh_1 = TestHandShake::ecdh(&secret_1, &pub_2.0.serialize());
            let ecdh_2 = TestHandShake::ecdh(&secret_2, &pub_1.0.serialize());

            if ecdh_1 == ecdh_2 {
                TestResult::passed()
            } else {
                TestResult::failed()
            }
        } else {
            TestResult::discard()
        }
    }
}
