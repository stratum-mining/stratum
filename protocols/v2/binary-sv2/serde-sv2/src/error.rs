use alloc::string::String;
use core::fmt::{self, Display};

use serde::{de, ser};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    // One or more variants that can be created by data structures through the
    // `ser::Error` and `de::Error` traits. For example the Serialize impl for
    // Mutex<T> might return an error because the mutex is poisoned, or the
    // Deserialize impl for a struct may return an error because a required
    // field is missing.
    InvalidBool(u8),
    InvalidBoolSize(usize),
    InvalidSignatureSize(usize),
    InvalidU16Size(usize),
    InvalidU24Size(usize),
    InvalidU32Size(usize),
    InvalidU64Size(usize),
    InvalidU256(usize),
    InvalidShortTxId(usize),
    InvalidUtf8,
    Message(String),
    LenBiggerThan16M,
    LenBiggerThan32,
    LenBiggerThan64K,
    LenBiggerThan255,
    ReadError,
    StringLenBiggerThan256,
    U24TooBig(u32),
    WriteError,
    PrimitiveConversionError,
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(format!("{}", msg))
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(format!("{}", msg))
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::InvalidBool(b) => formatter.write_fmt(format_args!(
                "Invalid Boolean. Expected Boolean value, got value of `{}`.",
                b
            )),
            Error::InvalidBoolSize(n) => {
                formatter.write_fmt(format_args!("Invalid Boolean. Expected Boolean size of 1 byte, got size of `{}` bytes.", n))
            }
            Error::InvalidSignatureSize(n) => formatter.write_fmt(format_args!(
                "Invalid signature. Expected signature size of 64 bytes, got size of `{}` bytes.",
                n
            )),
            Error::InvalidU16Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u16 number with size of 2 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidU24Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u24 number with size of 3 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidU32Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u32 number with size of 4 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidU64Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u64 number with size of 8 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidU256(n) => formatter.write_fmt(format_args!(
                "Invalid number. Expected u256 number with size of 32 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidShortTxId(n) => formatter.write_fmt(format_args!(
                "Invalid number. Expected ShortTxid number with size of 6 bytes, got number with size of `{}` bytes.",
                n
            )),
            Error::InvalidUtf8 => formatter.write_str("Invalid utf8."),
            Error::Message(msg) => formatter.write_str(msg),
            Error::LenBiggerThan16M => {
                formatter.write_str("Length is too long, must be less than 16M.")
            }
            Error::LenBiggerThan32 => {
                formatter.write_str("Length is too long, must be less than 32.")
            }
            Error::LenBiggerThan64K => {
                formatter.write_str("Length is too long, must be less than 64K.")
            }
            Error::LenBiggerThan255 => {
                formatter.write_str("Length is too long, must be less than 255.")
            }
            Error::ReadError => formatter.write_str("Read error."),
            Error::StringLenBiggerThan256 => {
                formatter.write_str("String length is too long, must be less than 256.")
            }
            Error::U24TooBig(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u24 number with size of 3 bytes, got number with size of `{}`.",
                n
            )),
            Error::WriteError => formatter.write_str("Write error."),
            Error::PrimitiveConversionError => formatter.write_str("Primitive conversion error."),
        }
    }
}
