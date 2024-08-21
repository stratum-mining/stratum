use crate::{aed_cipher::AeadCipher, cipher_state::CipherState, NOISE_HASHED_PROTOCOL_NAME_CHACHA};
use chacha20poly1305::ChaCha20Poly1305;
use secp256k1::{
    ecdh::SharedSecret,
    hashes::{sha256::Hash as Sha256Hash, Hash},
    rand, Keypair, Secp256k1, SecretKey, XOnlyPublicKey,
};

pub trait HandshakeOp<Cipher: AeadCipher>: CipherState<Cipher> {
    fn name(&self) -> String;
    fn get_h(&mut self) -> &mut [u8; 32];

    fn get_ck(&mut self) -> &mut [u8; 32];

    fn set_h(&mut self, data: [u8; 32]);

    fn set_ck(&mut self, data: [u8; 32]);

    fn mix_hash(&mut self, data: &[u8]) {
        let h = self.get_h();
        let mut to_hash = Vec::with_capacity(32 + data.len());
        to_hash.extend_from_slice(h);
        to_hash.extend_from_slice(data);
        *h = Sha256Hash::hash(&to_hash).to_byte_array();
    }

    fn generate_key() -> Keypair {
        let secp = Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut rand::thread_rng());
        let kp = Keypair::from_secret_key(&secp, &secret_key);
        if kp.x_only_public_key().1 == crate::PARITY {
            kp
        } else {
            Self::generate_key()
        }
    }

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

    fn hkdf_2(chaining_key: &[u8; 32], input_key_material: &[u8]) -> ([u8; 32], [u8; 32]) {
        let temp_key = Self::hmac_hash(chaining_key, input_key_material);
        let out_1 = Self::hmac_hash(&temp_key, &[0x1]);
        let out_2 = Self::hmac_hash(&temp_key, &[&out_1[..], &[0x2][..]].concat());
        (out_1, out_2)
    }

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

    fn mix_key(&mut self, input_key_material: &[u8]) {
        let ck = self.get_ck();
        let (ck, temp_k) = Self::hkdf_2(ck, input_key_material);
        self.set_ck(ck);
        self.initialize_key(temp_k);
    }

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

    fn ecdh(private: &[u8], public: &[u8]) -> [u8; 32] {
        let private = SecretKey::from_slice(private).expect("Wrong key");
        let x_public = XOnlyPublicKey::from_slice(public).expect("Wrong key");
        let res = SharedSecret::new(&x_public.public_key(crate::PARITY), &private);
        res.secret_bytes()
    }

    /// Prior to starting first round of NX-handshake, both initiator and responder initializes
    /// handshake variables h (hash output), ck (chaining key) and k (encryption key):
    fn initialize_self(&mut self) {
        let ck = NOISE_HASHED_PROTOCOL_NAME_CHACHA;
        let h = Sha256Hash::hash(&ck[..]);
        self.set_h(h.to_byte_array());
        self.set_ck(ck);
        self.set_k(None);
    }

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
    use quickcheck::{Arbitrary, TestResult};
    use quickcheck_macros;
    use std::convert::TryInto;

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
