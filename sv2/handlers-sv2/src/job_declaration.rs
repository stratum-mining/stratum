use binary_sv2::GetSize;
use core::convert::TryInto;
use extensions_sv2::{has_valid_tlv_data, Tlv};
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
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_server()`.
pub trait HandleJobDeclarationMessagesFromServerSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    fn handle_job_declaration_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        let parsed_size = parsed.get_size();

        // Check if there are remaining bytes that could be TLV data
        let tlv_fields = if raw_payload.len() > parsed_size {
            let remaining = &raw_payload[parsed_size..];
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id);

            // Validate and parse TLV data against negotiated extensions
            if has_valid_tlv_data(remaining, negotiated_extensions) {
                Some(Tlv::parse_all(remaining))
            } else {
                None
            }
        } else {
            None
        };

        self.handle_job_declaration_message_from_server(server_id, parsed, tlv_fields.as_deref())
    }

    /// Handles a parsed job declaration message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_job_declaration_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: JobDeclaration<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                self.handle_allocate_mining_job_token_success(server_id, msg, tlv_fields)
            }
            JobDeclaration::DeclareMiningJobSuccess(msg) => {
                self.handle_declare_mining_job_success(server_id, msg, tlv_fields)
            }
            JobDeclaration::DeclareMiningJobError(msg) => {
                self.handle_declare_mining_job_error(server_id, msg, tlv_fields)
            }
            JobDeclaration::ProvideMissingTransactions(msg) => {
                self.handle_provide_missing_transactions(server_id, msg, tlv_fields)
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
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions(
        &mut self,
        server_id: Option<usize>,
        msg: ProvideMissingTransactions,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing job declaration messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `server_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_server()`.
#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromServerAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    async fn handle_job_declaration_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            let parsed_size = parsed.get_size();

            // Check if there are remaining bytes that could be TLV data
            let tlv_fields = if raw_payload.len() > parsed_size {
                let remaining = &raw_payload[parsed_size..];
                let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id);

                // Validate and parse TLV data against negotiated extensions
                if has_valid_tlv_data(remaining, negotiated_extensions) {
                    Some(Tlv::parse_all(remaining))
                } else {
                    None
                }
            } else {
                None
            };

            self.handle_job_declaration_message_from_server(
                server_id,
                parsed,
                tlv_fields.as_deref(),
            )
            .await
        }
    }

    /// Handles a parsed job declaration message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_job_declaration_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: JobDeclaration<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobTokenSuccess(msg) => {
                    self.handle_allocate_mining_job_token_success(server_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::DeclareMiningJobSuccess(msg) => {
                    self.handle_declare_mining_job_success(server_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::DeclareMiningJobError(msg) => {
                    self.handle_declare_mining_job_error(server_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::ProvideMissingTransactions(msg) => {
                    self.handle_provide_missing_transactions(server_id, msg, tlv_fields)
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
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_success(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job_error(
        &mut self,
        server_id: Option<usize>,
        msg: DeclareMiningJobError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions(
        &mut self,
        server_id: Option<usize>,
        msg: ProvideMissingTransactions,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing job declaration messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_client()`.
pub trait HandleJobDeclarationMessagesFromClientSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    fn handle_job_declaration_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: JobDeclaration<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        let parsed_size = parsed.get_size();

        // Check if there are remaining bytes that could be TLV data
        let tlv_fields = if raw_payload.len() > parsed_size {
            let remaining = &raw_payload[parsed_size..];
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id);

            // Validate and parse TLV data against negotiated extensions
            if has_valid_tlv_data(remaining, negotiated_extensions) {
                Some(Tlv::parse_all(remaining))
            } else {
                None
            }
        } else {
            None
        };

        self.handle_job_declaration_message_from_client(client_id, parsed, tlv_fields.as_deref())
    }

    /// Handles a parsed job declaration message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_job_declaration_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: JobDeclaration<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            JobDeclaration::AllocateMiningJobToken(msg) => {
                self.handle_allocate_mining_job_token(client_id, msg, tlv_fields)
            }
            JobDeclaration::DeclareMiningJob(msg) => {
                self.handle_declare_mining_job(client_id, msg, tlv_fields)
            }
            JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                self.handle_provide_missing_transactions_success(client_id, msg, tlv_fields)
            }
            JobDeclaration::PushSolution(msg) => {
                self.handle_push_solution(client_id, msg, tlv_fields)
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
            JobDeclaration::ProvideMissingTransactions(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            )),
        }
    }

    fn handle_allocate_mining_job_token(
        &mut self,
        client_id: Option<usize>,
        msg: AllocateMiningJobToken,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_declare_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: DeclareMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_provide_missing_transactions_success(
        &mut self,
        client_id: Option<usize>,
        msg: ProvideMissingTransactionsSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_push_solution(
        &mut self,
        client_id: Option<usize>,
        msg: PushSolution,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing job declaration messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// Whether this is relevant or not depends on which object is implementing the trait, and whether
/// this contextual information is readily available or not. In cases where `client_id` is either
/// irrelevant or can be inferred without the context, this should always be `None`.
///
/// ## TLV Extension Support
///
/// The `tlv_data` parameter in message handlers contains validated TLV fields if the message has
/// extension data appended. TLV fields are only passed if they match negotiated extensions
/// returned by `get_negotiated_extensions_with_client()`.
#[trait_variant::make(Send)]
pub trait HandleJobDeclarationMessagesFromClientAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    async fn handle_job_declaration_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: JobDeclaration<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            let parsed_size = parsed.get_size();

            // Check if there are remaining bytes that could be TLV data
            let tlv_fields = if raw_payload.len() > parsed_size {
                let remaining = &raw_payload[parsed_size..];
                let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id);

                // Validate and parse TLV data against negotiated extensions
                if has_valid_tlv_data(remaining, negotiated_extensions) {
                    Some(Tlv::parse_all(remaining))
                } else {
                    None
                }
            } else {
                None
            };

            self.handle_job_declaration_message_from_client(
                client_id,
                parsed,
                tlv_fields.as_deref(),
            )
            .await
        }
    }

    /// Handles a parsed job declaration message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_job_declaration_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: JobDeclaration<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                JobDeclaration::AllocateMiningJobToken(msg) => {
                    self.handle_allocate_mining_job_token(client_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::DeclareMiningJob(msg) => {
                    self.handle_declare_mining_job(client_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::ProvideMissingTransactionsSuccess(msg) => {
                    self.handle_provide_missing_transactions_success(client_id, msg, tlv_fields)
                        .await
                }
                JobDeclaration::PushSolution(msg) => {
                    self.handle_push_solution(client_id, msg, tlv_fields).await
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
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_declare_mining_job(
        &mut self,
        client_id: Option<usize>,
        msg: DeclareMiningJob,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_provide_missing_transactions_success(
        &mut self,
        client_id: Option<usize>,
        msg: ProvideMissingTransactionsSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_push_solution(
        &mut self,
        client_id: Option<usize>,
        msg: PushSolution,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
