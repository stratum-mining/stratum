use stratum_core::bitcoin::{
    block::ValidationError, consensus::encode::Error as ConsensusEncodeError,
};

#[derive(Debug)]
pub enum TemplateDataError {
    InvalidCoinbaseTx(ConsensusEncodeError),
    InvalidSolution(ValidationError),
    InvalidMerkleRoot,
    CapnpError(capnp::Error),
    FailedIpcSubmitSolution,
}

impl From<ConsensusEncodeError> for TemplateDataError {
    fn from(error: ConsensusEncodeError) -> Self {
        TemplateDataError::InvalidCoinbaseTx(error)
    }
}

impl From<capnp::Error> for TemplateDataError {
    fn from(error: capnp::Error) -> Self {
        TemplateDataError::CapnpError(error)
    }
}

impl std::fmt::Display for TemplateDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateDataError::InvalidCoinbaseTx(e) => {
                write!(f, "Invalid coinbase transaction: {}", e)
            }
            TemplateDataError::InvalidSolution(e) => write!(f, "Invalid solution: {}", e),
            TemplateDataError::InvalidMerkleRoot => write!(f, "Invalid merkle root"),
            TemplateDataError::CapnpError(e) => write!(f, "Cap'n Proto error: {}", e),
            TemplateDataError::FailedIpcSubmitSolution => {
                write!(f, "Failed to submit solution via IPC")
            }
        }
    }
}

impl std::error::Error for TemplateDataError {}
