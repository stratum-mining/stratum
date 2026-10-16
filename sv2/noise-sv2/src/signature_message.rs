// # Signature Noise Message and Certificate
//
// Defines the [`SignatureNoiseMessage`] (spec 4.5.2 `SIGNATURE_NOISE_MESSAGE`), which the
// responder sends as the payload of the second handshake message, and verification of the
// certificate (spec 4.5.3 `CERTIFICATE`) it forms together with the responder's static public key.
//
// ## Usage
//
// The [`crate::Responder`] uses `sign_with_rng` to have its authority sign the certificate over
// the version, the validity window and its own static public key. The [`crate::Initiator`] uses
// `verify_with_now` to check that certificate against the authority public key it was given, if
// any, and the current time.

use core::convert::TryInto;
use core::fmt;

use secp256k1::{hashes::sha256, schnorr::Signature, Keypair, Message, Secp256k1, XOnlyPublicKey};

/// The 74-byte `SIGNATURE_NOISE_MESSAGE` (spec 4.5.2) the responder sends during the handshake.
///
/// It carries the certificate's version and validity window and the authority's Schnorr
/// signature over the certificate (spec 4.5.3), which also covers the responder's static public
/// key. The message alone authenticates nothing: only the certificate it forms with that key can
/// be verified against the authority key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureNoiseMessage {
    /// Version of the certificate format, see [`crate::CERTIFICATE_VERSION`].
    pub version: u16,
    /// Validity start time (unix timestamp).
    pub valid_from: u32,
    /// The signature is invalid after this point in time (unix timestamp).
    pub not_valid_after: u32,
    /// The authority's Schnorr signature over the certificate.
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

/// The check the responder's certificate failed during verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateError {
    /// The certificate version is not [`crate::CERTIFICATE_VERSION`].
    UnsupportedVersion,
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
    /// The certificate signature, over the version, the validity window and the responder's static
    /// public key, does not verify against the authority public key. Either key can be the cause,
    /// as can malformed signature bytes; the signature cannot tell them apart.
    InvalidSignature,
}

impl fmt::Display for CertificateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion => write!(
                f,
                "unsupported version, expected {}",
                crate::CERTIFICATE_VERSION
            ),
            Self::NotYetValid { now } => write!(f, "not yet valid, local clock reads {now}"),
            Self::Expired { now } => write!(f, "expired, local clock reads {now}"),
            Self::InvalidSignature => write!(
                f,
                "signature does not verify for this server static pubkey and authority pubkey"
            ),
        }
    }
}
impl From<[u8; 74]> for SignatureNoiseMessage {
    // Converts a 74-byte array into a [`SignatureNoiseMessage`].
    //
    // Allows a raw 74-byte array to be converted into a [`SignatureNoiseMessage`], extracting the
    // version, validity periods, and signature from the provided data. Panics if the byte array
    // cannot be correctly converted into the struct fields.
    fn from(value: [u8; 74]) -> Self {
        let version = u16::from_le_bytes(value[0..2].try_into().unwrap());
        let valid_from = u32::from_le_bytes(value[2..6].try_into().unwrap());
        let not_valid_after = u32::from_le_bytes(value[6..10].try_into().unwrap());
        let signature = value[10..74].try_into().unwrap();
        Self {
            version,
            valid_from,
            not_valid_after,
            signature,
        }
    }
}

impl SignatureNoiseMessage {
    /// Verifies the certificate this `SignatureNoiseMessage` forms with the responder's static key
    /// `pk`, at a given timestamp with 10 seconds of tolerance.
    ///
    /// With an authority public key, the message must carry the supported certificate version,
    /// its signature over the responder's static key `pk` must verify against the authority key,
    /// and `now` must fall inside its validity window. The checks run in that order, so
    /// [`CertificateError::NotYetValid`] and [`CertificateError::Expired`] are only reported for
    /// timestamps the authority actually signed.
    ///
    /// # Security
    ///
    /// Without an authority public key, only the certificate version is checked and the message is
    /// otherwise accepted unverified, so `Ok` does not mean the responder was authenticated.
    ///
    /// The current system time should be provided to avoid relying on `std` and allow `no_std`
    /// environments to use another source of time.
    #[inline]
    pub(crate) fn verify_with_now(
        self,
        pk: &XOnlyPublicKey,
        authority_pk: &Option<XOnlyPublicKey>,
        now: u32,
    ) -> Result<(), CertificateError> {
        // Allow the local clock to drift up to 10 seconds ahead or behind.
        // See https://github.com/stratum-mining/stratum/issues/2015
        const TIME_LEEWAY: u32 = 10;

        if self.version != crate::CERTIFICATE_VERSION {
            return Err(CertificateError::UnsupportedVersion);
        }

        let Some(authority_pk) = authority_pk else {
            return Ok(());
        };

        let secp = Secp256k1::verification_only();
        let (m, s) = self.split();
        // m = SHA-256(version || valid_from || not_valid_after || server_static_key)
        let m = [&m[0..10], &pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        Signature::from_slice(&s)
            .and_then(|s| secp.verify_schnorr(&s, &m, authority_pk))
            .map_err(|_| CertificateError::InvalidSignature)?;

        // Use saturating ops to cap edges (valid_from ≥ 0, not_valid_after ≤ u32::MAX),
        // preventing wrap-around and subtle validation bugs with untrusted timestamps.
        if self.valid_from.saturating_sub(TIME_LEEWAY) > now {
            return Err(CertificateError::NotYetValid { now });
        }
        if self.not_valid_after.saturating_add(TIME_LEEWAY) < now {
            return Err(CertificateError::Expired { now });
        }
        Ok(())
    }

    /// Has the authority keypair `kp` sign the certificate formed by the version and validity
    /// window already in `msg` and the responder's static key `static_pk`, writing the signature
    /// into `msg`.
    ///
    /// The random number generator is used in the signature generation. It should be provided in
    /// order to not implicitely rely on `std` and allow `no_std` environments to provide a
    /// hardware random number generator for example.
    #[inline]
    pub(crate) fn sign_with_rng<R: rand::Rng + rand::CryptoRng>(
        msg: &mut [u8; 74],
        static_pk: &XOnlyPublicKey,
        kp: &Keypair,
        rng: &mut R,
    ) {
        let secp = Secp256k1::signing_only();
        let m = [&msg[0..10], &static_pk.serialize()].concat();
        let m = Message::from_hashed_data::<sha256::Hash>(&m);
        let signature = secp.sign_schnorr_with_rng(&m, kp, rng);
        for (i, b) in signature.as_ref().iter().enumerate() {
            msg[10 + i] = *b;
        }
    }

    // Splits the [`SignatureNoiseMessage`] into its component parts: the message hash and the
    // signature.
    //
    // Separates the message into the first 10 bytes (containing the version and validity period)
    // and the 64-byte Schnorr signature, returning them in a tuple. Used internally during the
    // verification process.
    fn split(self) -> ([u8; 10], [u8; 64]) {
        let mut m = [0; 10];
        m[0] = self.version.to_le_bytes()[0];
        m[1] = self.version.to_le_bytes()[1];
        m[2] = self.valid_from.to_le_bytes()[0];
        m[3] = self.valid_from.to_le_bytes()[1];
        m[4] = self.valid_from.to_le_bytes()[2];
        m[5] = self.valid_from.to_le_bytes()[3];
        m[6] = self.not_valid_after.to_le_bytes()[0];
        m[7] = self.not_valid_after.to_le_bytes()[1];
        m[8] = self.not_valid_after.to_le_bytes()[2];
        m[9] = self.not_valid_after.to_le_bytes()[3];
        (m, self.signature)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[cfg(feature = "std")]
    #[cfg_attr(miri, ignore)]
    fn verify_reports_why_a_certificate_is_rejected() {
        use rand::{rngs::StdRng, SeedableRng};

        let secp = Secp256k1::new();
        let mut rng = StdRng::seed_from_u64(42);
        let authority = Keypair::new(&secp, &mut rng);
        let responder_static = Keypair::new(&secp, &mut rng);
        let pk = responder_static.x_only_public_key().0;
        let authority_pk = Some(authority.x_only_public_key().0);

        let mut signature_noise_message = |version: u16| {
            let mut encoded = [0u8; 74];
            encoded[0..2].copy_from_slice(&version.to_le_bytes());
            encoded[2..6].copy_from_slice(&100u32.to_le_bytes());
            encoded[6..10].copy_from_slice(&200u32.to_le_bytes());
            SignatureNoiseMessage::sign_with_rng(&mut encoded, &pk, &authority, &mut rng);
            SignatureNoiseMessage::from(encoded)
        };
        let valid = signature_noise_message(crate::CERTIFICATE_VERSION);
        let mut forged = valid;
        forged.signature[0] ^= 1;

        assert_eq!(valid.verify_with_now(&pk, &authority_pk, 150), Ok(()));
        assert_eq!(
            signature_noise_message(crate::CERTIFICATE_VERSION + 1).verify_with_now(
                &pk,
                &authority_pk,
                150
            ),
            Err(CertificateError::UnsupportedVersion)
        );
        assert_eq!(
            valid.verify_with_now(&pk, &authority_pk, 89),
            Err(CertificateError::NotYetValid { now: 89 })
        );
        assert_eq!(
            valid.verify_with_now(&pk, &authority_pk, 211),
            Err(CertificateError::Expired { now: 211 })
        );
        assert_eq!(
            forged.verify_with_now(&pk, &authority_pk, 150),
            Err(CertificateError::InvalidSignature)
        );
        // A certificate issued for another server static pubkey does not verify either.
        let other_pk = Keypair::new(&secp, &mut rng).x_only_public_key().0;
        assert_eq!(
            valid.verify_with_now(&other_pk, &authority_pk, 150),
            Err(CertificateError::InvalidSignature)
        );
        // A forged certificate outside its window is reported as forged, not as expired.
        assert_eq!(
            forged.verify_with_now(&pk, &authority_pk, 211),
            Err(CertificateError::InvalidSignature)
        );
    }
}
