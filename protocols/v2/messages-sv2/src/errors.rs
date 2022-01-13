use binary_sv2::Error as BinarySv2Error;

#[derive(Debug)]
pub enum Error {
    BinarySv2Error(BinarySv2Error),
    WrongMessageType(u8),
    UnexpectedMessage,
    // min_v max_v all falgs supported
    NoPairableUpstream((u16, u16, u32)),
}

impl From<BinarySv2Error> for Error {
    fn from(v: BinarySv2Error) -> Error {
        Error::BinarySv2Error(v)
    }
}
