use core::convert::TryInto;
use job_declaration_sv2::{
    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    MESSAGE_TYPE_DECLARE_MINING_JOB, MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
    MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS, MESSAGE_TYPE_PUSH_SOLUTION, *,
};
use parsers_sv2::JobDeclaration;

use crate::error::HandlerErrorType;

pub trait HandleJobDeclarationMessagesFromServerSync {
    type Error: HandlerErrorType;
    fn handle_job_declaration_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        server_id: usize,
    ) -> Result<(), Self::Error> {
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_job_declaration_message_from_server(parsed, server_id)
    }

    fn handle_job_declaration_message_from_server(
        &mut self,
        message: JobDeclaration<'_>,
        server_id: usize,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                self.handle_allocate_mining_job_token_success(msg, server_id)
            }
            JobDeclaration::DeclareMiningJobSuccess(msg) => {
                self.handle_declare_mining_job_success(msg, server_id)
            }
            JobDeclaration::DeclareMiningJobError(msg) => {
                self.handle_declare_mining_job_error(msg, server_id)
            }
            JobDeclaration::ProvideMissingTransactions(msg) => {
                self.handle_provide_missing_transactions(msg, server_id)
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
        msg: AllocateMiningJobTokenSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions,
        server_id: usize,
    ) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromServerAsync {
    type Error: HandlerErrorType;
    async fn handle_job_declaration_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        server_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_job_declaration_message_from_server(parsed, server_id)
                .await
        }
    }

    async fn handle_job_declaration_message_from_server(
        &mut self,
        message: JobDeclaration<'_>,
        server_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                    self.handle_allocate_mining_job_token_success(msg, server_id)
                        .await
                }
                JobDeclaration::DeclareMiningJobSuccess(msg) => {
                    self.handle_declare_mining_job_success(msg, server_id).await
                }
                JobDeclaration::DeclareMiningJobError(msg) => {
                    self.handle_declare_mining_job_error(msg, server_id).await
                }
                JobDeclaration::ProvideMissingTransactions(msg) => {
                    self.handle_provide_missing_transactions(msg, server_id)
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
        msg: AllocateMiningJobTokenSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_success(
        &mut self,
        msg: DeclareMiningJobSuccess,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_error(
        &mut self,
        msg: DeclareMiningJobError,
        server_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions(
        &mut self,
        msg: ProvideMissingTransactions,
        server_id: usize,
    ) -> Result<(), Self::Error>;
}

pub trait HandleJobDeclarationMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn handle_job_declaration_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        client_id: usize,
    ) -> Result<(), Self::Error> {
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_job_declaration_message_from_client(parsed, client_id)
    }

    fn handle_job_declaration_message_from_client(
        &mut self,
        message: JobDeclaration<'_>,
        client_id: usize,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobToken(msg) => {
                self.handle_allocate_mining_job_token(msg, client_id)
            }
            JobDeclaration::DeclareMiningJob(msg) => self.handle_declare_mining_job(msg, client_id),
            JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                self.handle_provide_missing_transactions_success(msg, client_id)
            }
            JobDeclaration::PushSolution(msg) => self.handle_push_solution(msg, client_id),

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
        msg: AllocateMiningJobToken,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job(
        &mut self,
        msg: DeclareMiningJob,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions_success(
        &mut self,
        msg: ProvideMissingTransactionsSuccess,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    fn handle_push_solution(
        &mut self,
        msg: PushSolution,
        client_id: usize,
    ) -> Result<(), Self::Error>;
}

#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromClientAsync {
    type Error: HandlerErrorType;

    async fn handle_job_declaration_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
        client_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_job_declaration_message_from_client(parsed, client_id)
                .await
        }
    }

    async fn handle_job_declaration_message_from_client(
        &mut self,
        message: JobDeclaration<'_>,
        client_id: usize,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobToken(msg) => {
                    self.handle_allocate_mining_job_token(msg, client_id).await
                }
                JobDeclaration::DeclareMiningJob(msg) => {
                    self.handle_declare_mining_job(msg, client_id).await
                }
                JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                    self.handle_provide_missing_transactions_success(msg, client_id)
                        .await
                }
                JobDeclaration::PushSolution(msg) => {
                    self.handle_push_solution(msg, client_id).await
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
        msg: AllocateMiningJobToken,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job(
        &mut self,
        msg: DeclareMiningJob,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions_success(
        &mut self,
        msg: ProvideMissingTransactionsSuccess,
        client_id: usize,
    ) -> Result<(), Self::Error>;

    async fn handle_push_solution(
        &mut self,
        msg: PushSolution,
        client_id: usize,
    ) -> Result<(), Self::Error>;
}
