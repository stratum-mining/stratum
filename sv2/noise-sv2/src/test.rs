use crate::{handshake::HandshakeOp, initiator::Initiator, responder::Responder};

#[test]
#[cfg(feature = "std")]
#[cfg_attr(miri, ignore)]
fn test_1() {
    let key_pair = Responder::generate_key();

    let mut initiator = Initiator::new(Some(key_pair.public_key().into()));
    let mut responder = Responder::new(key_pair, 31449600);
    let first_message = initiator.step_0().unwrap();
    let (second_message, mut responder_engine) = responder.step_1(first_message).unwrap();
    let mut initiator_engine = initiator.step_2(second_message).unwrap();
    let mut message = "ciao".as_bytes().to_vec();
    initiator_engine.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    responder_engine.decrypt(&mut message).unwrap();

    assert!(message == "ciao".as_bytes().to_vec());
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_1_with_rng() {
    let key_pair = Responder::generate_key_with_rng(&mut rand::thread_rng());

    let mut initiator: Box<Initiator> =
        Initiator::new_with_rng(Some(key_pair.public_key().into()), &mut rand::thread_rng());
    let mut responder = Responder::new_with_rng(key_pair, 31449600, &mut rand::thread_rng());
    let first_message = initiator.step_0().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    let (second_message, mut responder_engine) = responder
        .step_1_with_now_rng(first_message, now, &mut rand::thread_rng())
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    let mut initiator_engine = initiator.step_2_with_now(second_message, now).unwrap();
    let mut message = "ciao".as_bytes().to_vec();
    initiator_engine.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    responder_engine.decrypt(&mut message).unwrap();

    assert!(message == "ciao".as_bytes().to_vec());
}

#[test]
#[cfg(feature = "std")]
#[cfg_attr(miri, ignore)]
fn pre_issued_certificate_is_sent_verbatim() {
    use crate::{signature_message::SignatureNoiseMessage, Error};

    // Issued where the authority private key lives; the responder never sees that key.
    let authority = Responder::generate_key();
    let server_static = Responder::generate_key();
    let issue = |valid_from: u32, not_valid_after: u32| {
        SignatureNoiseMessage::sign(
            valid_from,
            not_valid_after,
            &server_static.x_only_public_key().0,
            &authority,
        )
    };
    let handshake = |certificate| {
        let mut initiator = Initiator::new(Some(authority.public_key().into()));
        let mut responder = Responder::from_certificate(server_static, certificate);
        let first_message = initiator.step_0().unwrap();
        let (second_message, responder_engine) = responder.step_1(first_message).unwrap();
        initiator
            .step_2(second_message)
            .map(|initiator_engine| (initiator_engine, responder_engine))
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    let (mut initiator_engine, mut responder_engine) =
        handshake(issue(now - 60, now + 60)).unwrap();
    let mut message = "ciao".as_bytes().to_vec();
    initiator_engine.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    responder_engine.decrypt(&mut message).unwrap();
    assert!(message == "ciao".as_bytes().to_vec());

    // The validity window travels inside the certificate: an expired one is sent as is, and the
    // initiator rejects it.
    assert!(matches!(
        handshake(issue(now - 120, now - 60)),
        Err(Error::InvalidCertificate(_))
    ));
}
