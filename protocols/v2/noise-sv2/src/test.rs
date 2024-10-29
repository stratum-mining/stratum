use crate::{handshake::HandshakeOp, initiator::Initiator, responder::Responder};

#[test]
fn test_1() {
    let key_pair = Responder::generate_key(&mut rand::thread_rng());

    let mut initiator = Initiator::new(Some(key_pair.public_key().into()), &mut rand::thread_rng());
    let mut responder = Responder::new(key_pair, 31449600, &mut rand::thread_rng());
    let first_message = initiator.step_0().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    let (second_message, mut codec_responder) = responder
        .step_1(first_message, now, &mut rand::thread_rng())
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    let mut codec_initiator = initiator.step_2(second_message, now).unwrap();
    let mut message = "ciao".as_bytes().to_vec();
    codec_initiator.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    codec_responder.decrypt(&mut message).unwrap();

    assert!(message == "ciao".as_bytes().to_vec());
}
