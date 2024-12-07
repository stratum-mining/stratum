// # Noise Protocol Handshake
//
// This example demonstrates how to use the `noise-sv2` crate to establish a Noise handshake
// between and initiator and responder, and encrypt and decrypt a secret message. It showcases how
// to:
//
// - Generate a cryptographic keypair using the `secp256k1` library.
// - Perform a Noise handshake between an initiator and responder.
// - Transition from handshake to secure communication mode.
// - Encrypt a message as the initiator role.
// - Decrypt the message as the responder role.
//
// ## Run
//
// ```sh
// cargo run --example handshake
// ```

use noise_sv2::{Initiator, Responder};
use secp256k1::{Keypair, Parity, Secp256k1};

// Even parity used in the Schnorr signature process
const PARITY: Parity = Parity::Even;
// Validity duration of the responder's certificate, seconds
const RESPONDER_CERT_VALIDITY: u32 = 3600;

// Generates a secp256k1 public/private key pair for the responder.
fn generate_key() -> Keypair {
    let secp = Secp256k1::new();
    let (secret_key, _) = secp.generate_keypair(&mut rand::thread_rng());
    let kp = Keypair::from_secret_key(&secp, &secret_key);
    if kp.x_only_public_key().1 == PARITY {
        kp
    } else {
        generate_key()
    }
}

fn main() {
    let mut secret_message = "Ciao, Mondo!".as_bytes().to_vec();

    let responder_key_pair = generate_key();

    #[cfg(feature = "std")]
    let mut initiator = Initiator::new(Some(responder_key_pair.public_key().into()));
    #[cfg(not(feature = "std"))]
    let mut initiator = Initiator::new_with_rng(
        Some(responder_key_pair.public_key().into()),
        &mut rand::thread_rng(),
    );
    #[cfg(feature = "std")]
    let mut responder = Responder::new(responder_key_pair, RESPONDER_CERT_VALIDITY);
    #[cfg(not(feature = "std"))]
    let mut responder = Responder::new_with_rng(
        responder_key_pair,
        RESPONDER_CERT_VALIDITY,
        &mut rand::thread_rng(),
    );

    let first_message = initiator
        .step_0()
        .expect("Initiator failed first step of handshake");

    #[cfg(feature = "std")]
    let (second_message, mut responder_state) = responder
        .step_1(first_message)
        .expect("Responder failed second step of handshake");
    #[cfg(not(feature = "std"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    #[cfg(not(feature = "std"))]
    let (second_message, mut responder_state) = responder
        .step_1_with_now_rng(first_message, now, &mut rand::thread_rng())
        .expect("Responder failed second step of handshake");

    #[cfg(feature = "std")]
    let mut initiator_state = initiator
        .step_2(second_message)
        .expect("Initiator failed third step of handshake");
    #[cfg(not(feature = "std"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    #[cfg(not(feature = "std"))]
    let mut initiator_state = initiator
        .step_2_with_now(second_message, now)
        .expect("Initiator failed third step of handshake");

    initiator_state
        .encrypt(&mut secret_message)
        .expect("Initiator failed to encrypt the secret message");
    assert!(secret_message != "Ciao, Mondo!".as_bytes().to_vec());

    responder_state
        .decrypt(&mut secret_message)
        .expect("Responder failed to decrypt the secret message");
    assert!(secret_message == "Ciao, Mondo!".as_bytes().to_vec());
}
