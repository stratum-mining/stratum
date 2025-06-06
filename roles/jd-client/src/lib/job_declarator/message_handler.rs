//! Job Declarator: Message handler module
//!
//! Handles upstream Job Declaration Protocol messages by implementing the
//! `ParseJobDeclarationMessagesFromUpstream` trait.
use super::JobDeclarator;
use roles_logic_sv2::{
    binary_sv2,
    handlers::{job_declaration::ParseJobDeclarationMessagesFromUpstream, SendTo_},
    job_declaration_sv2::{
        AllocateMiningJobTokenSuccess, DeclareMiningJobError, DeclareMiningJobSuccess,
        ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
    },
    parsers::JobDeclaration,
};
use tracing::{debug, error, info};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseJobDeclarationMessagesFromUpstream for JobDeclarator {
    /// Handles an `AllocateMiningJobTokenSuccess` message received from the JDS.
    ///
    /// This message indicates that the JDS has successfully allocated a mining job token
    /// in response to a previous `AllocateMiningJobToken` request. The allocated token
    /// is added to the `JobDeclarator`'s internal pool of available tokens.
    ///
    /// Returns `Ok(SendTo::None(None))` indicating that no immediate response
    /// is needed back to the JDS after successfully receiving and processing the token.
    fn handle_allocate_mining_job_token_success(
        &mut self,
        message: AllocateMiningJobTokenSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `AllocateMiningJobTokenSuccess` with id: {}",
            message.request_id
        );
        self.allocated_tokens.push(message.into_static());

        Ok(SendTo::None(None))
    }

    /// Handles a `DeclareMiningJobSuccess` message received from the JDS.
    ///
    /// Returns `Ok(SendTo::None(Some(message)))` wrapping the processed message
    /// for potential forwarding or further handling within the `JobDeclarator`'s
    /// message processing loop.
    fn handle_declare_mining_job_success(
        &mut self,
        message: DeclareMiningJobSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `DeclareMiningJobSuccess` with id {}",
            message.request_id
        );
        debug!("`DeclareMiningJobSuccess`: {:?}", message);
        let message = JobDeclaration::DeclareMiningJobSuccess(message.into_static());
        Ok(SendTo::None(Some(message)))
    }

    /// Handles a `DeclareMiningJobError` message received from the JDS.
    ///
    /// Returns `Ok(SendTo::None(None))` indicating that no immediate response is
    /// needed back to the JDS after receiving a job declaration error. The error
    /// has been logged.
    fn handle_declare_mining_job_error(
        &mut self,
        message: DeclareMiningJobError,
    ) -> Result<SendTo, Error> {
        error!(
            "Received `DeclareMiningJobError`, error code: {}",
            std::str::from_utf8(message.error_code.as_ref()).unwrap_or("unknown error code")
        );
        debug!("`DeclareMiningJobError`: {:?}", message);
        Ok(SendTo::None(None))
    }

    /// Handles a `ProvideMissingTransactions` message received from the JDS.
    ///
    /// This message is sent by the JDS to request the full transaction data for
    /// specific transactions that it has identified as missing based on previous
    /// communication (e.g., from an `IdentifyTransactions` exchange or a job declaration).
    ///
    /// The handler retrieves the full transaction list for the requested job (identified
    /// by `request_id`) from its `last_declare_mining_jobs_sent` window. It then filters
    /// this list to include only the transactions at the positions specified in the
    /// `unknown_tx_position_list` from the incoming message and constructs a
    /// `ProvideMissingTransactionsSuccess` message containing the requested transactions.
    ///
    /// Returns `Ok(SendTo::Respond(message_enum))` indicating that a response
    /// (`ProvideMissingTransactionsSuccess`) containing the requested transactions
    /// should be sent back to the JDS.
    fn handle_provide_missing_transactions(
        &mut self,
        message: ProvideMissingTransactions,
    ) -> Result<SendTo, Error> {
        info!(
            "Received `ProvideMissingTransactions` with id: {}",
            message.request_id
        );
        debug!("`ProvideMissingTransactions`: {:?}", message);

        // Find the corresponding declared job in the window using the request ID.
        // Extract the full transaction list from the found job's details.
        let tx_list = self
            .last_declare_mining_jobs_sent
            .iter()
            .find_map(|entry| {
                if let Some((id, last_declare_job)) = entry {
                    if *id == message.request_id {
                        Some(last_declare_job.clone().tx_list.into_inner())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .ok_or(Error::UnknownRequestId(message.request_id))?;

        // Get the list of positions for missing transactions.
        let unknown_tx_position_list: Vec<u16> = message.unknown_tx_position_list.into_inner();

        // Filter the full transaction list to get the missing transactions based on positions.
        let missing_transactions: Vec<binary_sv2::B016M> = unknown_tx_position_list
            .iter()
            .filter_map(|&pos| tx_list.get(pos as usize).cloned())
            .collect();
        let request_id = message.request_id;
        let message_provide_missing_transactions = ProvideMissingTransactionsSuccess {
            request_id,
            transaction_list: binary_sv2::Seq064K::new(missing_transactions).unwrap(),
        };
        let message_enum =
            JobDeclaration::ProvideMissingTransactionsSuccess(message_provide_missing_transactions);
        Ok(SendTo::Respond(message_enum))
    }
}
