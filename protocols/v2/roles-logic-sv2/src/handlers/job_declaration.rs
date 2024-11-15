//! # Job Declaration Handlers
//!
//! This module defines traits and functions for handling job declaration messages within the
//! Stratum V2 protocol. The job declaration process is integral to managing mining tasks and
//! transactions between server and client components.
//!
//! ## Core Traits
//!
//! - `ParseServerJobDeclarationMessages`: This trait is implemented by downstream nodes to process
//!   job declaration messages received from upstream nodes. The trait includes methods for handling
//!   job-related events such as mining job token allocation, job declaration successes or errors,
//!   and transaction identification or provisioning.
//! - `ParseClientJobDeclarationMessages`: This trait is implemented by upstream nodes to manage job
//!   declaration messages received from downstream nodes. It facilitates the handling of job
//!   declarations, mining job token allocation, and transaction solutions submitted by downstream
//!   nodes.
//!
//! ## Message Handling
//!
//! The handlers are responsible for the following tasks:
//! - Parsing and deserializing job declaration messages into appropriate types.
//! - Dispatching the deserialized messages to specific handler functions based on their type, such
//!   as handling job token allocation, job declaration success or error responses, and transaction
//!   data management.
//!
//! ## Return Type
//!
//! The functions return a `Result<SendTo, Error>`. The `SendTo` type determines the next action for
//! the message: whether the message should be relayed, responded to, or ignored. If an error occurs
//! during processing, the `Error` type is returned.
//!
//! ## Structure
//!
//! This module contains:
//! - Traits for processing job declaration messages, covering both server-side and client-side
//!   handling.
//! - Functions designed to parse, deserialize, and process messages related to job declarations,
//!   with robust error handling.
//! - Error handling mechanisms to address unexpected messages and ensure safe processing,
//!   particularly in the context of shared state.

use crate::{parsers::JobDeclaration, utils::Mutex};
use std::sync::Arc;
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use super::SendTo_;
use crate::errors::Error;
use core::convert::TryInto;
use job_declaration_sv2::*;
use tracing::{debug, error, info, trace};

/// A trait for parsing and handling SV2 job declaration messages sent by a server.
///
/// This trait is designed to be implemented by downstream components that need to handle
/// various job declaration messages from an upstream SV2 server, such as job allocation,
/// declaration success, and error messages.
pub trait ParseServerJobDeclarationMessages
where
    Self: Sized,
{
    /// Routes an incoming job declaration message to the appropriate handler function.
    ///
    /// # Parameters
    /// - `self_`: An `Arc<Mutex<Self>>` instance to ensure thread-safe access to the implementor.
    /// - `message_type`: The type identifier of the incoming message.
    /// - `payload`: A mutable slice containing the message payload.
    ///
    /// # Returns
    /// - `Ok(SendTo)`: Indicates the message was successfully handled.
    /// - `Err(Error)`: Indicates an error occurred during message parsing or handling.
    fn handle_message_job_declaration(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_job_declaration_deserialized(self_, (message_type, payload).try_into())
    }

    /// Routes a deserialized job declaration message to the appropriate handler function.
    ///
    /// # Parameters
    /// - `self_`: An `Arc<Mutex<Self>>` instance to ensure thread-safe access to the implementor.
    /// - `message`: A `Result<JobDeclaration, Error>` containing either the parsed message or an
    ///   error.
    ///
    /// # Returns
    /// - `Ok(SendTo)`: Indicates the message was successfully handled.
    /// - `Err(Error)`: Indicates an error occurred during message parsing or handling.
    fn handle_message_job_declaration_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<JobDeclaration<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(JobDeclaration::AllocateMiningJobTokenSuccess(message)) => {
                debug!(
                    "Received AllocateMiningJobTokenSuccess with id: {}",
                    message.request_id
                );
                trace!("AllocateMiningJobTokenSuccess: {:?}", message.request_id);
                self_
                    .safe_lock(|x| x.handle_allocate_mining_job_token_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJobSuccess(message)) => {
                info!(
                    "Received DeclareMiningJobSuccess with id {}",
                    message.request_id
                );
                debug!("DeclareMiningJobSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJobError(message)) => {
                error!(
                    "Received DeclareMiningJobError, error code: {}",
                    std::str::from_utf8(message.error_code.as_ref())
                        .unwrap_or("unknown error code")
                );
                debug!("DeclareMiningJobSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job_error(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::IdentifyTransactions(message)) => {
                info!(
                    "Received IdentifyTransactions with id: {}",
                    message.request_id
                );
                debug!("IdentifyTransactions: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_identify_transactions(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::ProvideMissingTransactions(message)) => {
                info!(
                    "Received ProvideMissingTransactions with id: {}",
                    message.request_id
                );
                debug!("ProvideMissingTransactions: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_provide_missing_transactions(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }

    /// Handles an `AllocateMiningJobTokenSuccess` message.
    ///
    /// This method processes a message indicating a successful job token allocation.
    ///
    /// # Arguments
    /// - `message`: The `AllocateMiningJobTokenSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `DeclareMiningJobSuccess` message.
    ///
    /// This method processes a message indicating a successful mining job declaration.
    ///
    /// # Arguments
    /// - `message`: The `DeclareMiningJobSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_declare_mining_job_success(
        &mut self,
        message: DeclareMiningJobSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `DeclareMiningJobError` message.
    ///
    /// This method processes a message indicating an error in the mining job declaration process.
    ///
    /// # Arguments
    /// - `message`: The `DeclareMiningJobError` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_declare_mining_job_error(
        &mut self,
        message: DeclareMiningJobError,
    ) -> Result<SendTo, Error>;

    /// Handles an `IdentifyTransactions` message.
    ///
    /// This method processes a message that provides transaction identification data.
    ///
    /// # Arguments
    /// - `message`: The `IdentifyTransactions` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_identify_transactions(
        &mut self,
        message: IdentifyTransactions,
    ) -> Result<SendTo, Error>;

    /// Handles a `ProvideMissingTransactions` message.
    ///
    /// This method processes a message that supplies missing transaction data.
    ///
    /// # Arguments
    /// - `message`: The `ProvideMissingTransactions` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error>;
}

/// The `ParseClientJobDeclarationMessages` trait is responsible for handling job declaration
/// messages sent by clients to upstream nodes. The methods process messages like job declarations,
/// solutions, and transaction success indicators, ensuring proper routing and handling.
pub trait ParseClientJobDeclarationMessages
where
    Self: Sized,
{
    /// Routes an incoming job declaration message to the appropriate handler function.
    ///
    /// # Parameters
    /// - `self_`: An `Arc<Mutex<Self>>` instance to ensure thread-safe access to the implementor.
    /// - `message_type`: The type identifier of the incoming message.
    /// - `payload`: A mutable slice containing the message payload.
    ///
    /// # Returns
    /// - `Ok(SendTo)`: Indicates the message was successfully handled.
    /// - `Err(Error)`: Indicates an error occurred during message parsing or handling.
    fn handle_message_job_declaration(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_job_declaration_deserialized(self_, (message_type, payload).try_into())
    }

    /// Routes a deserialized job declaration message to the appropriate handler function.
    ///
    /// # Parameters
    /// - `self_`: An `Arc<Mutex<Self>>` instance to ensure thread-safe access to the implementor.
    /// - `message`: A `Result<JobDeclaration, Error>` containing either the parsed message or an
    ///   error.
    ///
    /// # Returns
    /// - `Ok(SendTo)`: Indicates the message was successfully handled.
    /// - `Err(Error)`: Indicates an error occurred during message parsing or handling.
    fn handle_message_job_declaration_deserialized(
        self_: Arc<Mutex<Self>>,
        message: Result<JobDeclaration<'_>, Error>,
    ) -> Result<SendTo, Error> {
        match message {
            Ok(JobDeclaration::AllocateMiningJobToken(message)) => {
                debug!(
                    "Received AllocateMiningJobToken with id: {}",
                    message.request_id
                );
                trace!("AllocateMiningJobToken: {:?}", message.request_id);
                self_
                    .safe_lock(|x| x.handle_allocate_mining_job_token(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::DeclareMiningJob(message)) => {
                info!("Received DeclareMiningJob with id: {}", message.request_id);
                debug!("DeclareMiningJob: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_declare_mining_job(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::IdentifyTransactionsSuccess(message)) => {
                info!(
                    "Received IdentifyTransactionsSuccess with id: {}",
                    message.request_id
                );
                debug!("IdentifyTransactionsSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_identify_transactions_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::ProvideMissingTransactionsSuccess(message)) => {
                info!(
                    "Received ProvideMissingTransactionsSuccess with id: {}",
                    message.request_id
                );
                debug!("ProvideMissingTransactionsSuccess: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_provide_missing_transactions_success(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(JobDeclaration::SubmitSolution(message)) => {
                info!("Received SubmitSolution");
                debug!("SubmitSolution: {:?}", message);
                self_
                    .safe_lock(|x| x.handle_submit_solution(message))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }

    /// Handles an `AllocateMiningJobToken` message.
    ///
    /// This method processes a message that allocates a mining job token.
    ///
    /// # Arguments
    /// - `message`: The `AllocateMiningJobToken` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error>;

    /// Handles a `DeclareMiningJob` message.
    ///
    /// This method processes a message that declares a new mining job.
    ///
    /// # Arguments
    /// - `message`: The `DeclareMiningJob` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_declare_mining_job(&mut self, message: DeclareMiningJob) -> Result<SendTo, Error>;

    /// Handles an `IdentifyTransactionsSuccess` message.
    ///
    /// This method processes a message that confirms the identification of transactions.
    ///
    /// # Arguments
    /// - `message`: The `IdentifyTransactionsSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_identify_transactions_success(
        &mut self,
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `ProvideMissingTransactionsSuccess` message.
    ///
    /// This method processes a message that confirms the receipt of missing transactions.
    ///
    /// # Arguments
    /// - `message`: The `ProvideMissingTransactionsSuccess` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error>;

    /// Handles a `SubmitSolution` message.
    ///
    /// This method processes a message that submits a solution for the mining job.
    ///
    /// # Arguments
    /// - `message`: The `SubmitSolutionJd` message.
    ///
    /// # Returns
    /// - `Result<SendTo, Error>`: The result of processing the message.
    fn handle_submit_solution(&mut self, message: SubmitSolutionJd) -> Result<SendTo, Error>;
}
