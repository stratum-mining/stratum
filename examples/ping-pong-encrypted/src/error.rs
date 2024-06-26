#[derive(std::fmt::Debug)]
pub enum Error {
    Io(std::io::Error),
    CodecSv2(codec_sv2::Error),
    FramingSv2(codec_sv2::framing_sv2::Error),
    BinarySv2(binary_sv2::Error),
    NoiseSv2(noise_sv2::Error),
    NetworkHelpersSv2(network_helpers_sv2::Error),
    KeyUtils(key_utils::Error),
    Receiver,
    Sender,
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
        Error::CodecSv2(e)
    }
}

impl From<network_helpers_sv2::Error> for Error {
    fn from(e: network_helpers_sv2::Error) -> Error {
        Error::NetworkHelpersSv2(e)
    }
}

impl From<binary_sv2::Error> for Error {
    fn from(e: binary_sv2::Error) -> Error {
        Error::BinarySv2(e)
    }
}

impl From<noise_sv2::Error> for Error {
    fn from(e: noise_sv2::Error) -> Error {
        Error::NoiseSv2(e)
    }
}

impl From<key_utils::Error> for Error {
    fn from(e: key_utils::Error) -> Error {
        Error::KeyUtils(e)
    }
}

impl From<codec_sv2::framing_sv2::Error> for Error {
    fn from(e: codec_sv2::framing_sv2::Error) -> Error {
        Error::FramingSv2(e)
    }
}
