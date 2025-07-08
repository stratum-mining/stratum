//! # Template Distribution Handlers
//!
//! This module defines traits and functions for handling template distribution messages within the
//! Stratum V2 protocol.
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
use crate::{errors::Error, utils::Mutex};
use parsers_sv2::TemplateDistribution;
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

/// see [`SendTo_`]
pub type SendTo = SendTo_<TemplateDistribution<'static>, ()>;
use core::convert::TryInto;
use std::sync::Arc;
use template_distribution_sv2::*;

/// Trait for handling template distribution messages received from server (Template Provider).
/// Includes functions to handle messages such as new templates, previous hash updates, and
/// transaction data requests.
pub trait ParseTemplateDistributionMessagesFromServer
where
    Self: Sized,
{
    /// Handles incoming template distribution messages.
    ///
    /// This function is responsible for parsing and dispatching the appropriate handler based on
    /// the message type. It first deserializes the payload and then routes it to the
    /// corresponding handler function.
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_deserialized(
            self_,
            (message_type, payload).try_into().map_err(Into::into),
        )
    }

    /// Handles deserialized template distribution messages.
    ///
    /// This function takes the deserialized message and processes it according to the specific
    /// message type, invoking the appropriate handler function.
    fn handle_message_template_distribution_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match message {
            Ok(TemplateDistribution::NewTemplate(m)) => {
                self_.safe_lock(|x| x.handle_new_template(m))?
            }
            Ok(TemplateDistribution::SetNewPrevHash(m)) => {
                self_.safe_lock(|x| x.handle_set_new_prev_hash(m))?
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                self_.safe_lock(|x| x.handle_request_tx_data_success(m))?
            }
            Ok(TemplateDistribution::RequestTransactionDataError(m)) => {
                self_.safe_lock(|x| x.handle_request_tx_data_error(m))?
            }
            Ok(TemplateDistribution::CoinbaseOutputConstraints(_)) => Err(
                Error::UnexpectedMessage(MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
            ),
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
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error>;

    /// Handles a `SetNewPrevHash` message.
    ///
    /// This method processes the `SetNewPrevHash` message, which updates the previous hash for a
    /// template.
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionDataSuccess` message.
    ///
    /// This method processes the success response for a requested transaction data message.
    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionDataError` message.
    ///
    /// This method processes an error response for a requested transaction data message.
    fn handle_request_tx_data_error(
        &mut self,
        m: RequestTransactionDataError,
    ) -> Result<SendTo, Error>;
}

/// Trait for handling template distribution messages received from downstream nodes (client side).
/// Includes functions to handle messages such as coinbase output data size, transaction data
/// requests, and solution submissions.
pub trait ParseTemplateDistributionMessagesFromClient
where
    Self: Sized,
{
    /// Handles incoming template distribution messages.
    ///
    /// This function is responsible for parsing and dispatching the appropriate handler based on
    /// the message type. It first deserializes the payload and then routes it to the
    /// corresponding handler function.
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_deserialized(
            self_,
            (message_type, payload).try_into().map_err(Into::into),
        )
    }

    /// Handles deserialized template distribution messages.
    ///
    /// This function takes the deserialized message and processes it according to the specific
    /// message type, invoking the appropriate handler function.
    fn handle_message_template_distribution_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(TemplateDistribution::CoinbaseOutputConstraints(m)) => {
                self_.safe_lock(|x| x.handle_coinbase_out_data_size(m))?
            }
            Ok(TemplateDistribution::RequestTransactionData(m)) => {
                self_.safe_lock(|x| x.handle_request_tx_data(m))?
            }
            Ok(TemplateDistribution::SubmitSolution(m)) => {
                self_.safe_lock(|x| x.handle_request_submit_solution(m))?
            }
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

    /// Handles a `CoinbaseOutputConstraints` message.
    ///
    /// This method processes a message that includes the coinbase output data size.
    fn handle_coinbase_out_data_size(
        &mut self,
        m: CoinbaseOutputConstraints,
    ) -> Result<SendTo, Error>;

    /// Handles a `RequestTransactionData` message.
    ///
    /// This method processes a message requesting transaction data.
    fn handle_request_tx_data(&mut self, m: RequestTransactionData) -> Result<SendTo, Error>;

    /// Handles a `SubmitSolution` message.
    ///
    /// This method processes a solution submission message.
    fn handle_request_submit_solution(&mut self, m: SubmitSolution) -> Result<SendTo, Error>;
}
