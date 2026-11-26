// # Signature Noise Message and Certificate
//
// Defines the [`SignatureNoiseMessage`] (spec 4.5.2 `SIGNATURE_NOISE_MESSAGE`), which the
// responder sends as the payload of the second handshake message, and verification of the
// certificate (spec 4.5.3 `CERTIFICATE`) it forms together with the responder's static public key,
// kept as a [`Certificate`] once that verification succeeds.
//
// ## Usage
//
// Whoever holds the authority key, a [`crate::Responder`] signing at construction or a separate
// issuer, uses `sign` or `sign_with_rng` to produce the signature over the version, the validity
// window and the responder's static public key. The [`crate::Initiator`] uses `verify_with_now`
// to check that certificate against the authority public key it pins and the current time;
// whoever loads a pre-issued certificate can run the same check ahead of any handshake and learn
// which part of it failed.

use core::convert::TryInto;
use core::fmt;

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};

use crate::{CERTIFICATE_TIME_LEEWAY, CERTIFICATE_VERSION, SIGNATURE_NOISE_MESSAGE_SIZE};

// Size in bytes of the signed header: `version`, `valid_from` and `not_valid_after`.
const HEADER_SIZE: usize = 10;

/// The 74-byte `SIGNATURE_NOISE_MESSAGE` (spec 4.5.2) the responder sends during the handshake.
///
/// It carries the certificate's version and validity window and the authority's Schnorr
/// signature over the certificate (spec 4.5.3), which also covers the responder's static public
/// key. The message alone authenticates nothing: only the certificate it forms with that key can
/// be verified against the authority key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureNoiseMessage {
    /// Certificate format version; only [`crate::CERTIFICATE_VERSION`] verifies.
    pub version: u16,
    /// Start of the validity period for the message, expressed as a Unix timestamp.
    pub valid_from: u32,
    /// End of the validity period for the message, expressed as a Unix timestamp.
    pub not_valid_after: u32,
    /// 64-byte Schnorr signature that authenticates the message.
    pub signature: [u8; 64],
}

// The signature is left out: it means nothing to a human reading logs, and `Debug` still has it.
impl fmt::Display for SignatureNoiseMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "signature noise message version {}, valid from {} until {} (unix time)",
            self.version, self.valid_from, self.not_valid_after
        )
    }
}

/// The responder's certificate (spec 4.5.3 `CERTIFICATE`), which the handshake never sends as
/// such: it is formed by the [`SignatureNoiseMessage`] received during the handshake and the
/// server's static public key its signature covers, together with the authority public key that
/// signature was verified against.
///
/// Only a `Certificate` returned by [`crate::NoiseEngine::certificate`] is known to have been
/// verified; one built from its public fields carries no such guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Certificate {
    /// The `SIGNATURE_NOISE_MESSAGE` the responder sent.
    pub signature_noise_message: SignatureNoiseMessage,
    /// The responder's static public key used in the NX handshake, which the signature covers.
    pub server_static_pubkey: XOnlyPublicKey,
    /// The authority public key the signature was verified against; not covered by the signature.
    pub authority_pubkey: XOnlyPublicKey,
}

/// The check the responder's certificate failed during verification, in the order
/// [`SignatureNoiseMessage::verify_with_now`] runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateError {
    /// The certificate version is not [`CERTIFICATE_VERSION`].
    UnsupportedVersion,
    /// The certificate signature, over the version, the validity window and the responder's static
    /// public key, does not verify against the authority public key. Either key can be the cause,
    /// as can malformed signature bytes; the signature cannot tell them apart.
    InvalidSignature,
    /// The validity window ends before it starts, so it contains no time at all.
    EmptyValidityWindow,
    /// `now` (unix time) falls before the certificate's `valid_from`, beyond the clock leeway.
    NotYetValid {
        /// The clock reading the window was checked against (unix time).
        now: u32,
    },
    /// `now` (unix time) falls after the certificate's `not_valid_after`, beyond the clock leeway.
    Expired {
        /// The clock reading the window was checked against (unix time).
        now: u32,
    },
}

impl fmt::Display for CertificateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion => {
                write!(f, "unsupported version, expected {CERTIFICATE_VERSION}")
            }
            Self::InvalidSignature => write!(
                f,
                "signature does not verify for this server static pubkey and authority pubkey"
            ),
            Self::EmptyValidityWindow => write!(f, "validity window ends before it starts"),
            Self::NotYetValid { now } => write!(f, "not yet valid, local clock reads {now}"),
            Self::Expired { now } => write!(f, "expired, local clock reads {now}"),
        }
    }
}
impl From<SignatureNoiseMessage> for [u8; SIGNATURE_NOISE_MESSAGE_SIZE] {
    /// Encodes a [`SignatureNoiseMessage`] in the 74-byte layout the handshake carries: the
    /// little-endian `version`, `valid_from` and `not_valid_after`, then the signature. The inverse
    /// of the conversion from bytes.
    fn from(value: SignatureNoiseMessage) -> Self {
        let mut encoded = [0; SIGNATURE_NOISE_MESSAGE_SIZE];
        encoded[..HEADER_SIZE].copy_from_slice(&value.header());
        encoded[HEADER_SIZE..].copy_from_slice(&value.signature);
        encoded
    }
}

impl From<[u8; SIGNATURE_NOISE_MESSAGE_SIZE]> for SignatureNoiseMessage {
    /// Decodes the 74-byte layout the handshake carries: the little-endian `version`, `valid_from`
    /// and `not_valid_after`, then the signature.
    fn from(value: [u8; SIGNATURE_NOISE_MESSAGE_SIZE]) -> Self {
        let version = u16::from_le_bytes(value[0..2].try_into().unwrap());
        let valid_from = u32::from_le_bytes(value[2..6].try_into().unwrap());
        let not_valid_after = u32::from_le_bytes(value[6..HEADER_SIZE].try_into().unwrap());
        let signature = value[HEADER_SIZE..].try_into().unwrap();
        Self {
            version,
            valid_from,
            not_valid_after,
            signature,
        }
    }
}

impl SignatureNoiseMessage {
    /// Verifies the certificate this message forms with the responder's static public key
    /// `static_pk` against the authority public key `authority_pk` at the current system time.
    /// See [`Self::verify_with_now`].
    #[cfg(feature = "std")]
    pub fn verify(
        &self,
        static_pk: &XOnlyPublicKey,
        authority_pk: &XOnlyPublicKey,
    ) -> Result<(), CertificateError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        self.verify_with_now(static_pk, authority_pk, now)
    }

    /// Verifies the certificate this message forms with the responder's static public key
    /// `static_pk` against the authority public key `authority_pk`, at a given timestamp with
    /// [`CERTIFICATE_TIME_LEEWAY`] seconds of tolerance.
    ///
    /// The message must carry the supported certificate version, its signature over `static_pk`
    /// must verify against `authority_pk`, and `now` must fall inside its validity window. The
    /// checks run in that order and the first failure is returned, so a window problem is only
    /// ever reported for timestamps the authority signed: a forged certificate, or one issued for
    /// another static key or by another authority, is [`CertificateError::InvalidSignature`]
    /// whatever its timestamps.
    ///
    /// The current system time should be provided to avoid relying on `std` and allow `no_std`
    /// environments to use another source of time.
    #[inline]
    pub fn verify_with_now(
        &self,
        static_pk: &XOnlyPublicKey,
        authority_pk: &XOnlyPublicKey,
        now: u32,
    ) -> Result<(), CertificateError> {
        if self.version != CERTIFICATE_VERSION {
            return Err(CertificateError::UnsupportedVersion);
        }

        let secp = Secp256k1::verification_only();
        // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
        let m = [&self.header()[..], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        Signature::from_slice(&self.signature)
            .and_then(|s| secp.verify_schnorr(&s, &m, authority_pk))
            .map_err(|_| CertificateError::InvalidSignature)?;

        self.check_validity_window(now)
    }

    /// Whether `now` (Unix time) falls inside the validity window, with
    /// [`CERTIFICATE_TIME_LEEWAY`] seconds of tolerance on both ends. A window that ends before it
    /// starts contains no time at all, whatever the tolerance.
    ///
    /// This is the time check [`Self::verify_with_now`] applies. On its own it says nothing about
    /// who signed the message.
    #[inline]
    pub fn is_within_validity_window(&self, now: u32) -> bool {
        self.check_validity_window(now).is_ok()
    }

    // The time check of `verify_with_now`, which runs it only once the signature has verified.
    fn check_validity_window(&self, now: u32) -> Result<(), CertificateError> {
        if self.valid_from > self.not_valid_after {
            return Err(CertificateError::EmptyValidityWindow);
        }
        // Use saturating ops to cap edges (valid_from ≥ 0, not_valid_after ≤ u32::MAX),
        // preventing wrap-around and subtle validation bugs with untrusted timestamps.
        if self.valid_from.saturating_sub(CERTIFICATE_TIME_LEEWAY) > now {
            return Err(CertificateError::NotYetValid { now });
        }
        if now > self.not_valid_after.saturating_add(CERTIFICATE_TIME_LEEWAY) {
            return Err(CertificateError::Expired { now });
        }
        Ok(())
    }

    /// Issues a [`SignatureNoiseMessage`] of the supported certificate version for the server's
    /// static public key (`static_pk`), valid between the two Unix timestamps and signed with the
    /// `authority` keypair.
    #[cfg(feature = "std")]
    pub fn sign(
        valid_from: u32,
        not_valid_after: u32,
        static_pk: &XOnlyPublicKey,
        authority: &Keypair,
    ) -> Self {
        Self::sign_with_rng(
            valid_from,
            not_valid_after,
            static_pk,
            authority,
            &mut rand::thread_rng(),
        )
    }

    /// Issues a [`SignatureNoiseMessage`] using a custom random number generator.
    ///
    /// See [`Self::sign`] for more details.
    ///
    /// The random number generator is used in the signature generation. It should be provided in
    /// order to not implicitely rely on `std` and allow `no_std` environments to provide a
    /// hardware random number generator for example.
    #[inline]
    pub fn sign_with_rng<R: rand::Rng + rand::CryptoRng + ?Sized>(
        valid_from: u32,
        not_valid_after: u32,
        static_pk: &XOnlyPublicKey,
        authority: &Keypair,
        rng: &mut R,
    ) -> Self {
        let mut message = Self {
            version: CERTIFICATE_VERSION,
            valid_from,
            not_valid_after,
            signature: [0; 64],
        };
        let secp = Secp256k1::signing_only();
        // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
        let m = [&message.header()[..], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        message.signature = *secp
            .sign_schnorr_with_rng(&m, authority, &mut &mut *rng)
            .as_ref();
        message
    }

    // The signed header: the little-endian `version`, `valid_from` and `not_valid_after`.
    fn header(&self) -> [u8; HEADER_SIZE] {
        let mut header = [0; HEADER_SIZE];
        header[0..2].copy_from_slice(&self.version.to_le_bytes());
        header[2..6].copy_from_slice(&self.valid_from.to_le_bytes());
        header[6..HEADER_SIZE].copy_from_slice(&self.not_valid_after.to_le_bytes());
        header
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn byte_encoding_round_trips() {
        let mut encoded = [0u8; SIGNATURE_NOISE_MESSAGE_SIZE];
        for (i, b) in encoded.iter_mut().enumerate() {
            *b = i as u8;
        }
        let message = SignatureNoiseMessage::from(encoded);
        assert_eq!(message.version, u16::from_le_bytes([0, 1]));
        assert_eq!(message.valid_from, u32::from_le_bytes([2, 3, 4, 5]));
        assert_eq!(message.not_valid_after, u32::from_le_bytes([6, 7, 8, 9]));
        assert_eq!(<[u8; SIGNATURE_NOISE_MESSAGE_SIZE]>::from(message), encoded);
    }

    #[test]
    #[cfg(feature = "std")]
    #[cfg_attr(miri, ignore)]
    fn verify_reports_why_a_certificate_is_rejected() {
        use rand::{rngs::StdRng, SeedableRng};

        let secp = Secp256k1::new();
        let mut rng = StdRng::seed_from_u64(42);
        let authority = Keypair::new(&secp, &mut rng);
        let authority_pk = authority.x_only_public_key().0;
        let static_pk = Keypair::new(&secp, &mut rng).x_only_public_key().0;
        let valid =
            SignatureNoiseMessage::sign_with_rng(100, 200, &static_pk, &authority, &mut rng);
        // Signs `message` as the authority would, whatever its header says.
        let mut resign = |message: &mut SignatureNoiseMessage| {
            let m = [&message.header()[..], &static_pk.serialize()].concat();
            let m = Message::from_hashed_data::<sha256::Hash>(&m);
            message.signature = *secp
                .sign_schnorr_with_rng(&m, &authority, &mut rng)
                .as_ref();
        };

        assert_eq!(
            valid.verify_with_now(&static_pk, &authority_pk, 150),
            Ok(())
        );

        let mut unsupported = valid;
        unsupported.version = CERTIFICATE_VERSION + 1;
        resign(&mut unsupported);
        assert_eq!(
            unsupported.verify_with_now(&static_pk, &authority_pk, 150),
            Err(CertificateError::UnsupportedVersion)
        );

        let mut empty = valid;
        empty.valid_from = 201;
        resign(&mut empty);
        assert_eq!(
            empty.verify_with_now(&static_pk, &authority_pk, 200),
            Err(CertificateError::EmptyValidityWindow)
        );

        let too_early = 100 - CERTIFICATE_TIME_LEEWAY - 1;
        let too_late = 200 + CERTIFICATE_TIME_LEEWAY + 1;
        assert_eq!(
            valid.verify_with_now(&static_pk, &authority_pk, too_early),
            Err(CertificateError::NotYetValid { now: too_early })
        );
        assert_eq!(
            valid.verify_with_now(&static_pk, &authority_pk, too_late),
            Err(CertificateError::Expired { now: too_late })
        );

        let mut forged = valid;
        forged.signature[0] ^= 1;
        assert_eq!(
            forged.verify_with_now(&static_pk, &authority_pk, 150),
            Err(CertificateError::InvalidSignature)
        );
        // Issued for another static key, or checked against another authority.
        let other_pk = Keypair::new(&secp, &mut rng).x_only_public_key().0;
        assert_eq!(
            valid.verify_with_now(&other_pk, &authority_pk, 150),
            Err(CertificateError::InvalidSignature)
        );
        assert_eq!(
            valid.verify_with_now(&static_pk, &other_pk, 150),
            Err(CertificateError::InvalidSignature)
        );
        // Outside its window, a forged certificate is still reported as forged, not as expired.
        assert_eq!(
            forged.verify_with_now(&static_pk, &authority_pk, too_late),
            Err(CertificateError::InvalidSignature)
        );
    }

    #[test]
    fn validity_window_has_leeway_on_both_ends() {
        let message = SignatureNoiseMessage {
            version: CERTIFICATE_VERSION,
            valid_from: 1_000,
            not_valid_after: 2_000,
            signature: [0; 64],
        };

        assert!(!message.is_within_validity_window(1_000 - CERTIFICATE_TIME_LEEWAY - 1));
        assert!(message.is_within_validity_window(1_000 - CERTIFICATE_TIME_LEEWAY));
        assert!(message.is_within_validity_window(2_000 + CERTIFICATE_TIME_LEEWAY));
        assert!(!message.is_within_validity_window(2_000 + CERTIFICATE_TIME_LEEWAY + 1));
    }

    #[test]
    fn inverted_validity_window_is_never_valid() {
        // Without the explicit check the leeway would make the two bounds overlap at 1_000.
        let message = SignatureNoiseMessage {
            version: CERTIFICATE_VERSION,
            valid_from: 1_001,
            not_valid_after: 999,
            signature: [0; 64],
        };

        for now in 980..=1_020 {
            assert!(!message.is_within_validity_window(now));
        }
    }
}
