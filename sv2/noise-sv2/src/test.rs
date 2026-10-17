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
    let certificate = initiator_engine.certificate().unwrap();
    assert_eq!(certificate.authority_pubkey, key_pair.x_only_public_key().0);
    // The server static pubkey is the one the authority signed.
    assert_eq!(
        certificate.signature_noise_message.verify_with_now(
            &certificate.server_static_pubkey,
            &Some(certificate.authority_pubkey),
            certificate.signature_noise_message.valid_from,
        ),
        Ok(())
    );
    assert!(responder_engine.certificate().is_none());
    let mut message = "ciao".as_bytes().to_vec();
    initiator_engine.encrypt(&mut message).unwrap();
    assert!(message != "ciao".as_bytes().to_vec());
    responder_engine.decrypt(&mut message).unwrap();

    assert!(message == "ciao".as_bytes().to_vec());
}

#[test]
#[cfg(feature = "std")]
#[cfg_attr(miri, ignore)]
fn handshake_reports_why_the_certificate_was_rejected() {
    let wrong_authority = Responder::generate_key();
    let mut initiator = Initiator::new(Some(wrong_authority.public_key().into()));
    let mut responder = Responder::new(Responder::generate_key(), 31449600);
    let first_message = initiator.step_0().unwrap();
    let (second_message, _) = responder.step_1(first_message).unwrap();
    assert!(matches!(
        initiator.step_2(second_message),
        Err(crate::Error::InvalidCertificate {
            reason: crate::CertificateError::InvalidSignature,
            ..
        })
    ));
}

#[test]
#[cfg(feature = "std")]
#[cfg_attr(miri, ignore)]
fn unverified_session_exposes_no_certificate() {
    let mut initiator = Initiator::without_pk().unwrap();
    let mut responder = Responder::new(Responder::generate_key(), 31449600);
    let first_message = initiator.step_0().unwrap();
    let (second_message, _) = responder.step_1(first_message).unwrap();
    let initiator_engine = initiator.step_2(second_message).unwrap();
    assert!(initiator_engine.certificate().is_none());
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
