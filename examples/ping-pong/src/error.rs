#[derive(std::fmt::Debug)]
pub enum Error {
    Io(std::io::Error),
    Codec(codec_sv2::Error),
    BinarySv2(binary_sv2::Error),
    FrameHeader,
    FrameFromMessage,
    Nonce,
    WrongMessage,
    Tcp(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<codec_sv2::Error> for Error {
    fn from(e: codec_sv2::Error) -> Error {
        Error::Codec(e)
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Error {
        Error::BinarySv2(e)
    }
}
