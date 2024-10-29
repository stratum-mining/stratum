use ::key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use secp256k1::{rand, Keypair, Secp256k1};

fn generate_key<R: rand::Rng + ?Sized>(rng: &mut R) -> (Secp256k1SecretKey, Secp256k1PublicKey) {
    let secp = Secp256k1::new();
    let (secret_key, _) = secp.generate_keypair(rng);
    let kp = Keypair::from_secret_key(&secp, &secret_key);
    if kp.x_only_public_key().1 == secp256k1::Parity::Even {
        (
            Secp256k1SecretKey(kp.secret_key()),
            Secp256k1PublicKey(kp.x_only_public_key().0),
        )
    } else {
        generate_key(rng)
    }
}

fn main() {
    let (secret, public) = generate_key(&mut rand::thread_rng());
    let secret: String = secret.into();
    let public: String = public.into();
    println!("Secret Key: {}", secret);
    println!("Public Key: {}", public);
}
