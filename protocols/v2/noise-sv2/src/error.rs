use aes_gcm::Error as AesGcm;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    HandshakeNotFinalized,
    CipherListMustBeNonEmpty,
    UnsupportedCiphers(Vec<u8>),
    InvalidCipherList(Vec<u8>),
    InvalidCipherChosed(Vec<u8>),
    AesGcm(AesGcm),
    InvalidCipherState,
    InvalidCertificate([u8; 74]),
    InvalidRawPublicKey,
    InvalidRawPrivateKey,
    ExpectedIncomingHandshakeMessage,
    InvalidMessageLength,
}

impl From<AesGcm> for Error {
    fn from(value: AesGcm) -> Self {
        Self::AesGcm(value)
    }
}
