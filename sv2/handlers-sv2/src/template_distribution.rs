use binary_sv2::GetSize;
use core::convert::TryInto;
use extensions_sv2::{has_valid_tlv_data, Tlv};
use parsers_sv2::TemplateDistribution;
use template_distribution_sv2::*;
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing template distribution messages received from servers.
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
pub trait HandleTemplateDistributionMessagesFromServerSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: TemplateDistribution<'_> = (message_type, payload)
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

        self.handle_template_distribution_message_from_server(
            server_id,
            parsed,
            tlv_fields.as_deref(),
        )
    }

    /// Handles a parsed template distribution message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_template_distribution_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: TemplateDistribution<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistribution::NewTemplate(m) => {
                self.handle_new_template(server_id, m, tlv_fields)
            }
            TemplateDistribution::SetNewPrevHash(m) => {
                self.handle_set_new_prev_hash(server_id, m, tlv_fields)
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                self.handle_request_tx_data_success(server_id, m, tlv_fields)
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                self.handle_request_tx_data_error(server_id, m, tlv_fields)
            }

            TemplateDistribution::CoinbaseOutputConstraints(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
            ),
            TemplateDistribution::RequestTransactionData(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
            ),
            TemplateDistribution::SubmitSolution(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SUBMIT_SOLUTION,
            )),
        }
    }
    fn handle_new_template(
        &mut self,
        server_id: Option<usize>,
        msg: NewTemplate,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing template distribution messages received from servers.
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
pub trait HandleTemplateDistributionMessagesFromServerAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a server.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_server(&self, server_id: Option<usize>) -> &[u16];

    async fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: TemplateDistribution<'_> = (message_type, payload)
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

            self.handle_template_distribution_message_from_server(
                server_id,
                parsed,
                tlv_fields.as_deref(),
            )
            .await
        }
    }

    /// Handles a parsed template distribution message from a server.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_template_distribution_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: TemplateDistribution<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistribution::NewTemplate(m) => {
                    self.handle_new_template(server_id, m, tlv_fields).await
                }
                TemplateDistribution::SetNewPrevHash(m) => {
                    self.handle_set_new_prev_hash(server_id, m, tlv_fields)
                        .await
                }
                TemplateDistribution::RequestTransactionDataSuccess(m) => {
                    self.handle_request_tx_data_success(server_id, m, tlv_fields)
                        .await
                }
                TemplateDistribution::RequestTransactionDataError(m) => {
                    self.handle_request_tx_data_error(server_id, m, tlv_fields)
                        .await
                }

                TemplateDistribution::CoinbaseOutputConstraints(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
                ),
                TemplateDistribution::RequestTransactionData(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
                ),
                TemplateDistribution::SubmitSolution(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SUBMIT_SOLUTION,
                )),
            }
        }
    }
    async fn handle_new_template(
        &mut self,
        server_id: Option<usize>,
        msg: NewTemplate,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccess,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataError,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing template distribution messages received from clients.
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
pub trait HandleTemplateDistributionMessagesFromClientSync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let raw_payload = payload.to_vec();
        let parsed: TemplateDistribution<'_> = (message_type, payload)
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

        self.handle_template_distribution_message_from_client(
            client_id,
            parsed,
            tlv_fields.as_deref(),
        )
    }

    /// Handles a parsed template distribution message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    fn handle_template_distribution_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: TemplateDistribution<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                self.handle_coinbase_output_constraints(client_id, m, tlv_fields)
            }
            TemplateDistribution::RequestTransactionData(m) => {
                self.handle_request_tx_data(client_id, m, tlv_fields)
            }
            TemplateDistribution::SubmitSolution(m) => {
                self.handle_submit_solution(client_id, m, tlv_fields)
            }

            TemplateDistribution::NewTemplate(_) => {
                Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_TEMPLATE))
            }
            TemplateDistribution::SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )),
            TemplateDistribution::RequestTransactionDataSuccess(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            TemplateDistribution::RequestTransactionDataError(_) => Err(
                Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
            ),
        }
    }

    fn handle_coinbase_output_constraints(
        &mut self,
        client_id: Option<usize>,
        msg: CoinbaseOutputConstraints,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionData,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
    fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolution,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing template distribution messages received from clients.
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
pub trait HandleTemplateDistributionMessagesFromClientAsync {
    type Error: HandlerErrorType;

    /// Returns the list of negotiated extension_types with a client.
    ///
    /// Used to validate TLV fields appended to messages. Return an empty slice if no
    /// extensions have been negotiated.
    fn get_negotiated_extensions_with_client(&self, client_id: Option<usize>) -> &[u16];

    async fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let raw_payload = payload.to_vec();
            let parsed: TemplateDistribution<'_> = (message_type, payload)
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

            self.handle_template_distribution_message_from_client(
                client_id,
                parsed,
                tlv_fields.as_deref(),
            )
            .await
        }
    }

    /// Handles a parsed template distribution message from a client.
    ///
    /// The `tlv_fields` parameter contains parsed TLV fields if the message has extension
    /// data appended. It will be `Some(&[Tlv])` when valid TLV data is present, or `None`
    /// if no TLV data exists or validation fails. Each `Tlv` struct provides direct access to
    /// `extension_type`, `field_type`, `length`, and `value`.
    async fn handle_template_distribution_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: TemplateDistribution<'_>,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistribution::CoinbaseOutputConstraints(m) => {
                    self.handle_coinbase_output_constraints(client_id, m, tlv_fields)
                        .await
                }
                TemplateDistribution::RequestTransactionData(m) => {
                    self.handle_request_tx_data(client_id, m, tlv_fields).await
                }
                TemplateDistribution::SubmitSolution(m) => {
                    self.handle_submit_solution(client_id, m, tlv_fields).await
                }

                TemplateDistribution::NewTemplate(_) => {
                    Err(Self::Error::unexpected_message(MESSAGE_TYPE_NEW_TEMPLATE))
                }
                TemplateDistribution::SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                    MESSAGE_TYPE_SET_NEW_PREV_HASH,
                )),
                TemplateDistribution::RequestTransactionDataSuccess(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
                ),
                TemplateDistribution::RequestTransactionDataError(_) => Err(
                    Self::Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
                ),
            }
        }
    }

    async fn handle_coinbase_output_constraints(
        &mut self,
        client_id: Option<usize>,
        msg: CoinbaseOutputConstraints,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionData,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
    async fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolution,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
