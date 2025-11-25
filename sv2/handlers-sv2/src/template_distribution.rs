use framing_sv2::header::Header;
use parsers_sv2::{parse_message_frame_with_tlvs, AnyMessage, TemplateDistribution, Tlv};
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Template Distribution protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_template_distribution_message_from_server` with the parsed message and TLV fields.
    fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: TemplateDistribution<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_template_distribution_message_from_server(server_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::TemplateDistribution(parsed) => self
                .handle_template_distribution_message_from_server(
                    server_id,
                    parsed,
                    tlv_fields.as_deref(),
                ),
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
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
                Self::Error::unexpected_message(0, MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
            ),
            TemplateDistribution::RequestTransactionData(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
            ),
            TemplateDistribution::SubmitSolution(_) => Err(Self::Error::unexpected_message(
                0,
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_server(
        &self,
        server_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Template Distribution protocol message frame from a server.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_template_distribution_message_from_server` with the parsed message and TLV fields.
    async fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_server(server_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: TemplateDistribution<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_template_distribution_message_from_server(server_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::TemplateDistribution(parsed) => {
                    self.handle_template_distribution_message_from_server(
                        server_id,
                        parsed,
                        tlv_fields.as_deref(),
                    )
                    .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
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
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
                ),
                TemplateDistribution::RequestTransactionData(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
                ),
                TemplateDistribution::SubmitSolution(_) => Err(Self::Error::unexpected_message(
                    0,
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Template Distribution protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_template_distribution_message_from_client` with the parsed message and TLV fields.
    fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
        if negotiated_extensions.is_empty() {
            let parsed: TemplateDistribution<'_> = (header.msg_type(), payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            return self.handle_template_distribution_message_from_client(client_id, parsed, None);
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::TemplateDistribution(parsed) => self
                .handle_template_distribution_message_from_client(
                    client_id,
                    parsed,
                    tlv_fields.as_deref(),
                ),
            _ => Err(Self::Error::unexpected_message(
                header.ext_type_without_channel_msg(),
                header.msg_type(),
            )),
        }
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

            TemplateDistribution::NewTemplate(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_NEW_TEMPLATE,
            )),
            TemplateDistribution::SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )),
            TemplateDistribution::RequestTransactionDataSuccess(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            TemplateDistribution::RequestTransactionDataError(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
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
    /// Return an empty Vec if no extensions have been negotiated.
    fn get_negotiated_extensions_with_client(
        &self,
        client_id: Option<usize>,
    ) -> Result<Vec<u16>, Self::Error>;

    /// Handles a raw Template Distribution protocol message frame from a client.
    ///
    /// This method parses the raw frame, extracts any TLV extension data, and delegates
    /// to `handle_template_distribution_message_from_client` with the parsed message and TLV fields.
    async fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        header: Header,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let negotiated_extensions = self.get_negotiated_extensions_with_client(client_id)?;
            if negotiated_extensions.is_empty() {
                let parsed: TemplateDistribution<'_> = (header.msg_type(), payload)
                    .try_into()
                    .map_err(Self::Error::parse_error)?;
                return self
                    .handle_template_distribution_message_from_client(client_id, parsed, None)
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::TemplateDistribution(parsed) => {
                    self.handle_template_distribution_message_from_client(
                        client_id,
                        parsed,
                        tlv_fields.as_deref(),
                    )
                    .await
                }
                _ => Err(Self::Error::unexpected_message(
                    header.ext_type_without_channel_msg(),
                    header.msg_type(),
                )),
            }
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

                TemplateDistribution::NewTemplate(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_TEMPLATE,
                )),
                TemplateDistribution::SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_SET_NEW_PREV_HASH,
                )),
                TemplateDistribution::RequestTransactionDataSuccess(_) => {
                    Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
                    ))
                }
                TemplateDistribution::RequestTransactionDataError(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
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
