use alloc::string::String;
use core::fmt::{self, Display};

use serde::{de, ser};

pub type Result<T> = core::result::Result<T, Error>;

// TODO provode additional information in the error type:
// 1. byte offset into the input
// 2. ??
#[derive(Clone, Debug, PartialEq)]
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
                "Invalid Boolean. Expected Boolean, got `{}`.",
                b
            )),
            Error::InvalidBoolSize(n) => {
                formatter.write_fmt(format_args!("Invalid Boolean size: `{}`.", n))
            }
            Error::InvalidSignatureSize(n) => {
                formatter.write_fmt(format_args!("Invalid signature size: `{}`.", n))
            }
            Error::InvalidU16Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u16 number, got number with size of `{}`.",
                n
            )),
            Error::InvalidU24Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u24 number, got number with size of `{}`.",
                n
            )),
            Error::InvalidU32Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u32 number, got number with size of `{}`.",
                n
            )),
            Error::InvalidU64Size(n) => formatter.write_fmt(format_args!(
                "Invalid size. Expected u64 number, got number with size of `{}`.",
                n
            )),
            Error::InvalidU256(n) => formatter.write_fmt(format_args!(
                "Invalid number. Expected type u256, got `{}`.",
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
                "Invalid size. Expected u24 number, got number with size of `{}`.",
                n
            )),
            Error::WriteError => formatter.write_str("Write error."),
        }
    }
}

// impl core::error::Error for Error {}
