use core::convert::TryInto;
use job_declaration_sv2::{
    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    MESSAGE_TYPE_DECLARE_MINING_JOB, MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
    MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS, MESSAGE_TYPE_PUSH_SOLUTION, *,
};
use parsers_sv2::JobDeclaration;

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing job declaration messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
pub trait HandleJobDeclarationMessagesFromServerSync {
    type Error: HandlerErrorType;
    fn handle_job_declaration_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_job_declaration_message_from_server(server_id, parsed)
    }

    fn handle_job_declaration_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: JobDeclaration<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                self.handle_allocate_mining_job_token_success(server_id, msg)
            }
            JobDeclaration::DeclareMiningJobSuccess(msg) => {
                self.handle_declare_mining_job_success(server_id, msg)
            }
            JobDeclaration::DeclareMiningJobError(msg) => {
                self.handle_declare_mining_job_error(server_id, msg)
            }
            JobDeclaration::ProvideMissingTransactions(msg) => {
                self.handle_provide_missing_transactions(server_id, msg)
            }
            JobDeclaration::AllocateMiningJobToken(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
            )),
            JobDeclaration::DeclareMiningJob(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_DECLARE_MINING_JOB,
            )),
            JobDeclaration::ProvideMissingTransactionsSuccess(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS),
            ),
            JobDeclaration::PushSolution(_) => {
                Err(Self::Error::unexpected_message(MESSAGE_TYPE_PUSH_SOLUTION))
            }
        }
    }

    fn handle_allocate_mining_job_token_success(
        &mut self,
        server_id: Option<usize>,
        msg: AllocateMiningJobTokenSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobError,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions(
        &mut self,
        server_id: Option<usize>,
        msg: ProvideMissingTransactions,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing job declaration messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromServerAsync {
    type Error: HandlerErrorType;
    async fn handle_job_declaration_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_job_declaration_message_from_server(server_id, parsed)
                .await
        }
    }

    async fn handle_job_declaration_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: JobDeclaration<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                    self.handle_allocate_mining_job_token_success(server_id, msg)
                        .await
                }
                JobDeclaration::DeclareMiningJobSuccess(msg) => {
                    self.handle_declare_mining_job_success(server_id, msg).await
                }
                JobDeclaration::DeclareMiningJobError(msg) => {
                    self.handle_declare_mining_job_error(server_id, msg).await
                }
                JobDeclaration::ProvideMissingTransactions(msg) => {
                    self.handle_provide_missing_transactions(server_id, msg)
                        .await
                }
                JobDeclaration::AllocateMiningJobToken(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
                )),
                JobDeclaration::DeclareMiningJob(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_DECLARE_MINING_JOB,
                )),
                JobDeclaration::ProvideMissingTransactionsSuccess(_) => {
                    Err(Self::Error::unexpected_message(
                        MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
                    ))
                }
                JobDeclaration::PushSolution(_) => {
                    Err(Self::Error::unexpected_message(MESSAGE_TYPE_PUSH_SOLUTION))
                }
            }
        }
    }

    async fn handle_allocate_mining_job_token_success(
        &mut self,
        server_id: Option<usize>,
        msg: AllocateMiningJobTokenSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobError,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions(
        &mut self,
        server_id: Option<usize>,
        msg: ProvideMissingTransactions,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing job declaration messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
pub trait HandleJobDeclarationMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn handle_job_declaration_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_job_declaration_message_from_client(client_id, parsed)
    }

    fn handle_job_declaration_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: JobDeclaration<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobToken(msg) => {
                self.handle_allocate_mining_job_token(client_id, msg)
            }
            JobDeclaration::DeclareMiningJob(msg) => self.handle_declare_mining_job(client_id, msg),
            JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                self.handle_provide_missing_transactions_success(client_id, msg)
            }
            JobDeclaration::PushSolution(msg) => self.handle_push_solution(client_id, msg),

            JobDeclaration::AllocateMiningJobTokenSuccess(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS),
            ),
            JobDeclaration::DeclareMiningJobSuccess(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
            )),
            JobDeclaration::DeclareMiningJobError(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
            )),
            JobDeclaration::ProvideMissingTransactions(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            )),
        }
    }

    fn handle_allocate_mining_job_token(
        &mut self,
        client_id: Option<usize>,
        msg: AllocateMiningJobToken,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: DeclareMiningJob,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions_success(
        &mut self,
        client_id: Option<usize>,
        msg: ProvideMissingTransactionsSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_push_solution(
        &mut self,
        client_id: Option<usize>,
        msg: PushSolution,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing job declaration messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromClientAsync {
    type Error: HandlerErrorType;

    async fn handle_job_declaration_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_job_declaration_message_from_client(client_id, parsed)
                .await
        }
    }

    async fn handle_job_declaration_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: JobDeclaration<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobToken(msg) => {
                    self.handle_allocate_mining_job_token(client_id, msg).await
                }
                JobDeclaration::DeclareMiningJob(msg) => {
                    self.handle_declare_mining_job(client_id, msg).await
                }
                JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                    self.handle_provide_missing_transactions_success(client_id, msg)
                        .await
                }
                JobDeclaration::PushSolution(msg) => {
                    self.handle_push_solution(client_id, msg).await
                }

                JobDeclaration::AllocateMiningJobTokenSuccess(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS),
                ),
                JobDeclaration::DeclareMiningJobSuccess(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
                )),
                JobDeclaration::DeclareMiningJobError(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
                )),
                JobDeclaration::ProvideMissingTransactions(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS),
                ),
            }
        }
    }

    async fn handle_allocate_mining_job_token(
        &mut self,
        client_id: Option<usize>,
        msg: AllocateMiningJobToken,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: DeclareMiningJob,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions_success(
        &mut self,
        client_id: Option<usize>,
        msg: ProvideMissingTransactionsSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_push_solution(
        &mut self,
        client_id: Option<usize>,
        msg: PushSolution,
    ) -> Result<(), Self::Error>;
}
