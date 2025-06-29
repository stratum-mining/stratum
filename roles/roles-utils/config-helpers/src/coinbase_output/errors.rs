use core::fmt;

use miniscript::bitcoin::{address, hex};

/// Error enum
#[derive(Debug)]
pub enum Error {
    /// Error parsing a Bitcoin address
    Address(address::ParseError),
    // TODO rust-miniscript 13 will have functions to do these checks for us so we don't
    // need to pollute our own error enum with this fiddly stuff
    /// addr() descriptor did not have exactly 1 child
    AddrDescriptorNChildren(usize),
    /// raw() descriptor child did not have 0 children
    AddrDescriptorGrandchild,
    /// raw() descriptor did not have exactly 1 child
    RawDescriptorNChildren(usize),
    /// addr() descriptor child did not have 0 children
    RawDescriptorGrandchild,
    /// Error parsing a raw descriptor as hex.
    Hex(hex::HexToBytesError),
    /// Invalid `output_script_value` for script type. It must be a valid public key/script
    InvalidOutputScript,
    /// Unknown script type in config
    UnknownOutputScriptType,
    /// Error from the `miniscript` crate.
    Miniscript(miniscript::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Error::*;
        match self {
            Address(ref e) => write!(f, "Bitcoin address: {e}"),
            AddrDescriptorNChildren(0) => write!(f, "Found addr() descriptor with no address"),
            AddrDescriptorNChildren(n) => write!(f, "Found addr() descriptor with {n} children; must be exactly one valid address"),
            AddrDescriptorGrandchild => write!(f, "Found descriptor of the form addr(X(y)); X must be a valid address and have no subexpression"),
            RawDescriptorNChildren(0) => write!(f, "Found raw() descriptor with no hex-encoded script"),
            RawDescriptorNChildren(n) => write!(f, "Found raw() descriptor with {n} children; must be exactly one hex-encoded script"),
            RawDescriptorGrandchild => write!(f, "Found descriptor of the form raw(X(y)); X must be a hex-encoded script and have no subexpression"),
            Hex(ref e) => write!(f, "Decoding hex-formatted script: {e}"),
            UnknownOutputScriptType => write!(f, "Unknown script type in config"),
            InvalidOutputScript => write!(f, "Invalid output_script_value for your script type. It must be a valid public key/script"),
            Miniscript(ref e) => write!(f, "Miniscript: {e}"),
        }
    }
}

impl From<address::ParseError> for Error {
    fn from(e: address::ParseError) -> Self {
        Error::Address(e)
    }
}

impl From<hex::HexToBytesError> for Error {
    fn from(e: hex::HexToBytesError) -> Self {
        Error::Hex(e)
    }
}

impl From<miniscript::Error> for Error {
    fn from(e: miniscript::Error) -> Self {
        Error::Miniscript(e)
    }
}
