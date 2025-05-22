use core::fmt;

/// Error enum
#[derive(Debug)]
pub enum Error {
    /// Empty coinbase outputs in config
    EmptyCoinbaseOutputs,
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
            EmptyCoinbaseOutputs => write!(f, "Empty coinbase outputs in config"),
            UnknownOutputScriptType => write!(f, "Unknown script type in config"),
            InvalidOutputScript => write!(f, "Invalid output_script_value for your script type. It must be a valid public key/script"),
            Miniscript(ref e) => write!(f, "Miniscript: {e}"),
        }
    }
}

impl From<miniscript::Error> for Error {
    fn from(e: miniscript::Error) -> Self {
        Error::Miniscript(e)
    }
}
