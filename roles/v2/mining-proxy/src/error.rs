use std::{
    fmt::{self, Display, Formatter},
    path::PathBuf,
};

#[derive(Debug)]
pub enum Error {
    IoError {
        io_error: std::io::Error,
        path: PathBuf,
    },
    TomlError {
        toml_error: toml::de::Error,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;

        match self {
            IoError { io_error, path } => {
                write!(f, "I/O error at `{}: {}", path.display(), io_error)
            }
            TomlError { toml_error } => {
                write!(f, "Failed to parse toml file: `{}`", toml_error)
            }
        }
    }
}
