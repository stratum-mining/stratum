use crate::{handshake::HandshakeOp, initiator::Initiator, responder::Responder};

#[test]
fn test_1() {
    let key_pair = Responder::generate_key();

    let mut initiator = Initiator::new(key_pair.public_key().into());
    let mut responder = Responder::new(key_pair, 31449600);
    let first_message = initiator.step_0().unwrap();
    let (second_message, mut codec_responder) = responder.step_1(first_message).unwrap();
    let mut codec_initiator = initiator.step_2(second_message).unwrap();
    let mut message = "ciao".as_bytes().to_vec();
    codec_initiator.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    codec_responder.decrypt(&mut message).unwrap();

    assert!(message == "ciao".as_bytes().to_vec());
}
