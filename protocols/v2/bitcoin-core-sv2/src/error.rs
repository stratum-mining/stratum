use std::path::Path;
use stratum_core::bitcoin::consensus;

/// Error type for [`crate::BitcoinCoreSv2`]
#[derive(Debug)]
pub enum BitcoinCoreSv2Error {
    CapnpError(capnp::Error),
    CannotConnectToUnixSocket(Box<Path>),
    InvalidTemplateHeader(consensus::encode::Error),
    InvalidTemplateHeaderLength,
    FailedToSerializeCoinbasePrefix,
    FailedToSerializeCoinbaseOutputs,
    TemplateNotFound,
    TemplateIpcClientNotFound,
    FailedToSendNewTemplateMessage,
    FailedToSendSetNewPrevHashMessage,
    FailedToSendRequestTransactionDataResponseMessage,
    FailedToRecvTemplateDistributionMessage,
    FailedToSendTemplateDistributionMessage,
    FailedToSubmitSolution,
}

impl From<capnp::Error> for BitcoinCoreSv2Error {
    fn from(error: capnp::Error) -> Self {
        BitcoinCoreSv2Error::CapnpError(error)
    }
}

impl From<consensus::encode::Error> for BitcoinCoreSv2Error {
    fn from(error: consensus::encode::Error) -> Self {
        BitcoinCoreSv2Error::InvalidTemplateHeader(error)
    }
}
