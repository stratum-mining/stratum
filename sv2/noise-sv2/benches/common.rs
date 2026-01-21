use rand::{rngs::StdRng, SeedableRng};
use secp256k1::{Keypair, Secp256k1};

pub fn rng() -> StdRng {
    // Fixed seed for deterministic benchmark runs.
    StdRng::seed_from_u64(0xdead_beef)
}

// Generates BIP340 compliant keypair
pub fn generate_key_with_rng<R: rand::Rng + ?Sized>(rng: &mut R) -> Keypair {
    let secp = Secp256k1::new();
    let (mut secret_key, public_key) = secp.generate_keypair(rng);
    if public_key.x_only_public_key().1 == secp256k1::Parity::Odd {
        secret_key = secret_key.negate();
    }
    Keypair::from_secret_key(&secp, &secret_key)
}

#[allow(warnings)]
pub fn payload(len: usize) -> Vec<u8> {
    vec![0u8; len]
}
