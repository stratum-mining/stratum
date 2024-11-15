//! # Template Distribution Handlers
//!
//! This module defines traits and functions for handling template distribution messages within the
//! Stratum V2 protocol.
//!
//! ## Core Traits
//!
//! - `ParseServerTemplateDistributionMessages`: Implemented by downstream nodes to process template
//!   distribution messages received from upstream nodes. This trait includes methods for handling
//!   template-related events like new templates, previous hash updates, and transaction data
//!   requests.
//! - `ParseClientTemplateDistributionMessages`: Implemented by upstream nodes to manage template
//!   distribution messages received from downstream nodes. This trait handles coinbase output size,
//!   transaction data requests, and solution submissions.
//!
//! ## Message Handling
//!
//! Handlers are responsible for:
//! - Parsing and deserializing template distribution messages into appropriate types.
//! - Dispatching the deserialized messages to specific handler functions based on message type,
//!   such as handling new templates, transaction data requests, and coinbase output data.
//!
//! ## Return Type
//!
//! Functions return `Result<SendTo, Error>`, where `SendTo` determines the next action for the
//! message: whether it should be relayed, responded to, or ignored.
//!
//! ## Structure
//!
//! This module includes:
//! - Traits for processing template distribution messages, including server-side and client-side
//!   handling.
//! - Functions to parse, deserialize, and process messages related to template distribution,
//!   ensuring robust error handling.
//! - Error handling mechanisms to address unexpected messages and ensure safe processing,
//!   especially in the context of shared state.

use super::SendTo_;
use crate::{errors::Error, parsers::TemplateDistribution, utils::Mutex};
use template_distribution_sv2::{
    CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

pub type SendTo = SendTo_<TemplateDistribution<'static>, ()>;
use const_sv2::*;
use core::convert::TryInto;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

/// Trait for handling template distribution messages received from upstream nodes (server side).
/// Includes functions to handle messages such as new templates, previous hash updates, and
/// transaction data requests.
pub trait ParseServerTemplateDistributionMessages
where
    Self: Sized,
{
    /// Handles incoming template distribution messages.
    ///
    /// This function is responsible for parsing and dispatching the appropriate handler based on
    /// the message type. It first deserializes the payload and then routes it to the
    /// corresponding handler function.
    ///
    /// # Arguments
    /// - `self_`: An `Arc<Mutex<Self>>` representing the instance of the object implementing this
    ///   trait.
    /// - `message_type`: The type of the incoming message.
    /// - `payload`: The raw payload data of the message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message, where `SendTo` indicates
    ///   the next step in message handling.
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_desrialized(
            self_,
            (message_type, payload).try_into(),
        )
    }

    /// Handles deserialized template distribution messages.
    ///
    /// This function takes the deserialized message and processes it according to the specific
    /// message type, invoking the appropriate handler function.
    ///
    /// # Arguments
    /// - `self_`: An `Arc<Mutex<Self>>` representing the instance of the object implementing this
    ///   trait.
    /// - `message`: The deserialized `TemplateDistribution` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_message_template_distribution_desrialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match message {
            Ok(TemplateDistribution::NewTemplate(m)) => {
                info!(
                    "Received NewTemplate with id: {}, is future: {}",
                    m.template_id, m.future_template
                );
                debug!("NewTemplate: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_new_template(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::SetNewPrevHash(m)) => {
                info!("Received SetNewPrevHash for template: {}", m.template_id);
                debug!("SetNewPrevHash: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_set_new_prev_hash(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                info!(
                    "Received RequestTransactionDataSuccess for template: {}",
                    m.template_id
                );
                trace!("RequestTransactionDataSuccess: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_request_tx_data_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::RequestTransactionDataError(m)) => {
                error!(
                    "Received RequestTransactionDataError for template: {}, error: {}",
                    m.template_id,
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                self_
                    .safe_lock(|x| x.handle_request_tx_data_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::CoinbaseOutputDataSize(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE,
            )),
            Ok(TemplateDistribution::RequestTransactionData(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
            )),
            Ok(TemplateDistribution::SubmitSolution(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SUBMIT_SOLUTION))
            }
            Err(e) => Err(e),
        }
    }

    /// Handles a `NewTemplate` message.
    ///
    /// This method processes the `NewTemplate` message, which contains information about a newly
    /// generated template.
    ///
    /// # Arguments
    /// - `m`: The `NewTemplate` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error>;

    /// Handles a `SetNewPrevHash` message.
    ///
    /// This method processes the `SetNewPrevHash` message, which updates the previous hash for a
    /// template.
    ///
    /// # Arguments
    /// - `m`: The `SetNewPrevHash` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionDataSuccess` message.
    ///
    /// This method processes the success response for a requested transaction data message.
    ///
    /// # Arguments
    /// - `m`: The `RequestTransactionDataSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionDataError` message.
    ///
    /// This method processes an error response for a requested transaction data message.
    ///
    /// # Arguments
    /// - `m`: The `RequestTransactionDataError` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_request_tx_data_error(
        &mut self,
        m: RequestTransactionDataError,
    ) -> Result<SendTo, Error>;
}

/// Trait for handling template distribution messages received from downstream nodes (client side).
/// Includes functions to handle messages such as coinbase output data size, transaction data
/// requests, and solution submissions.
pub trait ParseClientTemplateDistributionMessages
where
    Self: Sized,
{
    /// Handles incoming template distribution messages.
    ///
    /// This function is responsible for parsing and dispatching the appropriate handler based on
    /// the message type. It first deserializes the payload and then routes it to the
    /// corresponding handler function.
    ///
    /// # Arguments
    /// - `self_`: An `Arc<Mutex<Self>>` representing the instance of the object implementing this
    ///   trait.
    /// - `message_type`: The type of the incoming message.
    /// - `payload`: The raw payload data of the message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message, where `SendTo` indicates
    ///   the next step in message handling.
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_desrialized(
            self_,
            (message_type, payload).try_into(),
        )
    }

    /// Handles deserialized template distribution messages.
    ///
    /// This function takes the deserialized message and processes it according to the specific
    /// message type, invoking the appropriate handler function.
    ///
    /// # Arguments
    /// - `self_`: An `Arc<Mutex<Self>>` representing the instance of the object implementing this
    ///   trait.
    /// - `message`: The deserialized `TemplateDistribution` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_message_template_distribution_desrialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match message {
            Ok(TemplateDistribution::CoinbaseOutputDataSize(m)) => self_
                .safe_lock(|x| x.handle_coinbase_out_data_size(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::RequestTransactionData(m)) => self_
                .safe_lock(|x| x.handle_request_tx_data(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::SubmitSolution(m)) => self_
                .safe_lock(|x| x.handle_request_submit_solution(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::NewTemplate(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_TEMPLATE))
            }
            Ok(TemplateDistribution::SetNewPrevHash(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_NEW_PREV_HASH))
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(_)) => Err(
                Error::UnexpectedMessage(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            Ok(TemplateDistribution::RequestTransactionDataError(_)) => Err(
                Error::UnexpectedMessage(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
            ),
            Err(e) => Err(e),
        }
    }

    /// Handles a `CoinbaseOutputDataSize` message.
    ///
    /// This method processes a message that includes the coinbase output data size.
    ///
    /// # Arguments
    /// - `m`: The `CoinbaseOutputDataSize` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_coinbase_out_data_size(&mut self, m: CoinbaseOutputDataSize)
        -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionData` message.
    ///
    /// This method processes a message requesting transaction data.
    ///
    /// # Arguments
    /// - `m`: The `RequestTransactionData` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_request_tx_data(&mut self, m: RequestTransactionData) -> Result<SendTo, Error>;

    /// Handles a `SubmitSolution` message.
    ///
    /// This method processes a solution submission message.
    ///
    /// # Arguments
    /// - `m`: The `SubmitSolution` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_request_submit_solution(&mut self, m: SubmitSolution) -> Result<SendTo, Error>;
}
