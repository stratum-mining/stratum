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

// impl core::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bool() {
        let invalid_bool = [2; 1];
        let actual = format!("{}", Error::InvalidBool(invalid_bool[0]));
        let expect = "Invalid Boolean. Expected Boolean value, got value of `2`.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_bool_size() {
        let invalid_bool = [0; 2];
        let actual = format!("{}", Error::InvalidBoolSize(invalid_bool.len()));
        let expect = "Invalid Boolean. Expected Boolean size of 1 byte, got size of `2` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_signature_size() {
        let invalid_signature = [0; 65];
        let actual = format!("{}", Error::InvalidSignatureSize(invalid_signature.len()));
        let expect =
            "Invalid signature. Expected signature size of 64 bytes, got size of `65` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_u16_size() {
        let invalid_u16 = [0; 3];
        let actual = format!("{}", Error::InvalidU16Size(invalid_u16.len()));
        let expect =
            "Invalid size. Expected u16 number with size of 2 bytes, got number with size of `3` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_u24_size() {
        let invalid_u24 = [0; 4];
        let actual = format!("{}", Error::InvalidU24Size(invalid_u24.len()));
        let expect =
            "Invalid size. Expected u24 number with size of 3 bytes, got number with size of `4` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_u32_size() {
        let invalid_u32 = [0; 5];
        let actual = format!("{}", Error::InvalidU32Size(invalid_u32.len()));
        let expect =
            "Invalid size. Expected u32 number with size of 4 bytes, got number with size of `5` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_u64_size() {
        let invalid_u64 = [0; 9];
        let actual = format!("{}", Error::InvalidU64Size(invalid_u64.len()));
        let expect =
            "Invalid size. Expected u64 number with size of 8 bytes, got number with size of `9` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn invalid_u256() {
        let invalid_u256 = [0; 33];
        let actual = format!("{}", Error::InvalidU256(invalid_u256.len()));
        let expect =
            "Invalid number. Expected u256 number with size of 32 bytes, got number with size of `33` bytes.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn message() {
        let actual = format!("{}", Error::Message(String::from("msg")));
        let expect = "msg";
        assert_eq!(expect, actual);
    }

    #[test]
    fn len_bigger_than_16m() {
        let actual = format!("{}", Error::LenBiggerThan16M);
        let expect = "Length is too long, must be less than 16M.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn len_bigger_than_32() {
        let actual = format!("{}", Error::LenBiggerThan32);
        let expect = "Length is too long, must be less than 32.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn len_bigger_than_64k() {
        let actual = format!("{}", Error::LenBiggerThan64K);
        let expect = "Length is too long, must be less than 64K.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn len_bigger_than_255() {
        let actual = format!("{}", Error::LenBiggerThan255);
        let expect = "Length is too long, must be less than 255.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn read_error() {
        let actual = format!("{}", Error::ReadError);
        let expect = "Read error.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn string_len_bigger_than_256() {
        let actual = format!("{}", Error::StringLenBiggerThan256);
        let expect = "String length is too long, must be less than 256.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn u24_too_big() {
        let invalid_u24 = 4294967295;
        let actual = format!("{}", Error::U24TooBig(invalid_u24));
        let expect = "Invalid size. Expected u24 number with size of 3 bytes, got number with size of `4294967295`.";
        assert_eq!(expect, actual);
    }

    #[test]
    fn write_error() {
        let actual = format!("{}", Error::WriteError);
        let expect = "Write error.";
        assert_eq!(expect, actual);
    }
}
