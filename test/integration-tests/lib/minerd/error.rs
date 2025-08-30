use std::fmt;

/// Errors that can occur when using MinerdWrapper
#[derive(Debug)]
pub enum MinerdError {
    /// IO operation failed
    Io(tokio::io::Error),
    /// Process spawn failed
    ProcessSpawn(tokio::io::Error),
    /// Process is already running
    ProcessAlreadyRunning,
    /// Process is not running when expected
    ProcessNotRunning,
    /// Network connection failed
    NetworkConnection(tokio::io::Error),
    /// Proxy setup failed
    ProxySetup(tokio::io::Error),
    /// Invalid configuration
    InvalidConfiguration(String),
    /// Failed to parse hashrate from minerd benchmark output
    HashrateParseError,
    /// Mutex was poisoned
    MutexPoisoned,
    /// OS or Architecture not supported
    OsArchNotSupported(String),
}

impl fmt::Display for MinerdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MinerdError::Io(e) => write!(f, "IO error: {}", e),
            MinerdError::ProcessSpawn(e) => write!(f, "Failed to spawn minerd process: {}", e),
            MinerdError::ProcessAlreadyRunning => write!(f, "Minerd process is already running"),
            MinerdError::ProcessNotRunning => write!(f, "Minerd process is not running"),
            MinerdError::NetworkConnection(e) => write!(f, "Network connection failed: {}", e),
            MinerdError::ProxySetup(e) => write!(f, "Proxy setup failed: {}", e),
            MinerdError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            MinerdError::HashrateParseError => {
                write!(f, "Failed to parse hashrate from minerd benchmark output")
            }
            MinerdError::MutexPoisoned => write!(f, "Mutex was poisoned"),
            MinerdError::OsArchNotSupported(msg) => {
                write!(f, "OS or architecture not supported: {}", msg)
            }
        }
    }
}

impl std::error::Error for MinerdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MinerdError::Io(e) => Some(e),
            MinerdError::ProcessSpawn(e) => Some(e),
            MinerdError::NetworkConnection(e) => Some(e),
            MinerdError::ProxySetup(e) => Some(e),
            MinerdError::ProcessAlreadyRunning
            | MinerdError::ProcessNotRunning
            | MinerdError::InvalidConfiguration(_)
            | MinerdError::HashrateParseError
            | MinerdError::MutexPoisoned => None,
            MinerdError::OsArchNotSupported(_) => None,
        }
    }
}

impl From<tokio::io::Error> for MinerdError {
    fn from(error: tokio::io::Error) -> Self {
        MinerdError::Io(error)
    }
}
