use framing_sv2::header::Header;
use parsers_sv2::{
    parse_message_frame_with_tlvs, AnyMessage, TemplateDistribution, TemplateDistributionOwned, Tlv,
};
use template_distribution_sv2::*;

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
pub trait HandleTemplateDistributionMessagesFromServerOwnedSync {
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
            return self.handle_template_distribution_message_from_server(
                server_id,
                parsed.into_owned(),
                None,
            );
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::TemplateDistribution(parsed) => self
                .handle_template_distribution_message_from_server(
                    server_id,
                    parsed.into_owned(),
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
        message: TemplateDistributionOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistributionOwned::NewTemplate(m) => {
                self.handle_new_template(server_id, m, tlv_fields)
            }
            TemplateDistributionOwned::SetNewPrevHash(m) => {
                self.handle_set_new_prev_hash(server_id, m, tlv_fields)
            }
            TemplateDistributionOwned::RequestTransactionDataSuccess(m) => {
                self.handle_request_tx_data_success(server_id, m, tlv_fields)
            }
            TemplateDistributionOwned::RequestTransactionDataError(m) => {
                self.handle_request_tx_data_error(server_id, m, tlv_fields)
            }

            TemplateDistributionOwned::CoinbaseOutputConstraints(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
            ),
            TemplateDistributionOwned::RequestTransactionData(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
            ),
            TemplateDistributionOwned::SubmitSolution(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SUBMIT_SOLUTION,
            )),
        }
    }
    fn handle_new_template(
        &mut self,
        server_id: Option<usize>,
        msg: NewTemplateOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHashOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccessOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataErrorOwned,
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
pub trait HandleTemplateDistributionMessagesFromServerOwnedAsync {
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
                    .handle_template_distribution_message_from_server(
                        server_id,
                        parsed.into_owned(),
                        None,
                    )
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::TemplateDistribution(parsed) => {
                    self.handle_template_distribution_message_from_server(
                        server_id,
                        parsed.into_owned(),
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
        message: TemplateDistributionOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistributionOwned::NewTemplate(m) => {
                    self.handle_new_template(server_id, m, tlv_fields).await
                }
                TemplateDistributionOwned::SetNewPrevHash(m) => {
                    self.handle_set_new_prev_hash(server_id, m, tlv_fields)
                        .await
                }
                TemplateDistributionOwned::RequestTransactionDataSuccess(m) => {
                    self.handle_request_tx_data_success(server_id, m, tlv_fields)
                        .await
                }
                TemplateDistributionOwned::RequestTransactionDataError(m) => {
                    self.handle_request_tx_data_error(server_id, m, tlv_fields)
                        .await
                }

                TemplateDistributionOwned::CoinbaseOutputConstraints(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
                ),
                TemplateDistributionOwned::RequestTransactionData(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA),
                ),
                TemplateDistributionOwned::SubmitSolution(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_SUBMIT_SOLUTION),
                ),
            }
        }
    }
    async fn handle_new_template(
        &mut self,
        server_id: Option<usize>,
        msg: NewTemplateOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHashOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccessOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataErrorOwned,
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
pub trait HandleTemplateDistributionMessagesFromClientOwnedSync {
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
            return self.handle_template_distribution_message_from_client(
                client_id,
                parsed.into_owned(),
                None,
            );
        }
        let (parsed, tlv_fields) =
            parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                .map_err(Self::Error::parse_error)?;
        match parsed {
            AnyMessage::TemplateDistribution(parsed) => self
                .handle_template_distribution_message_from_client(
                    client_id,
                    parsed.into_owned(),
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
        message: TemplateDistributionOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistributionOwned::CoinbaseOutputConstraints(m) => {
                self.handle_coinbase_output_constraints(client_id, m, tlv_fields)
            }
            TemplateDistributionOwned::RequestTransactionData(m) => {
                self.handle_request_tx_data(client_id, m, tlv_fields)
            }
            TemplateDistributionOwned::SubmitSolution(m) => {
                self.handle_submit_solution(client_id, m, tlv_fields)
            }

            TemplateDistributionOwned::NewTemplate(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_NEW_TEMPLATE,
            )),
            TemplateDistributionOwned::SetNewPrevHash(_) => Err(Self::Error::unexpected_message(
                0,
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )),
            TemplateDistributionOwned::RequestTransactionDataSuccess(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            TemplateDistributionOwned::RequestTransactionDataError(_) => Err(
                Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
            ),
        }
    }

    fn handle_coinbase_output_constraints(
        &mut self,
        client_id: Option<usize>,
        msg: CoinbaseOutputConstraintsOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionDataOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
    fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolutionOwned,
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
pub trait HandleTemplateDistributionMessagesFromClientOwnedAsync {
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
                    .handle_template_distribution_message_from_client(
                        client_id,
                        parsed.into_owned(),
                        None,
                    )
                    .await;
            }
            let (parsed, tlv_fields) =
                parse_message_frame_with_tlvs(header, payload, &negotiated_extensions)
                    .map_err(Self::Error::parse_error)?;
            match parsed {
                AnyMessage::TemplateDistribution(parsed) => {
                    self.handle_template_distribution_message_from_client(
                        client_id,
                        parsed.into_owned(),
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
        message: TemplateDistributionOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistributionOwned::CoinbaseOutputConstraints(m) => {
                    self.handle_coinbase_output_constraints(client_id, m, tlv_fields)
                        .await
                }
                TemplateDistributionOwned::RequestTransactionData(m) => {
                    self.handle_request_tx_data(client_id, m, tlv_fields).await
                }
                TemplateDistributionOwned::SubmitSolution(m) => {
                    self.handle_submit_solution(client_id, m, tlv_fields).await
                }

                TemplateDistributionOwned::NewTemplate(_) => Err(Self::Error::unexpected_message(
                    0,
                    MESSAGE_TYPE_NEW_TEMPLATE,
                )),
                TemplateDistributionOwned::SetNewPrevHash(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_SET_NEW_PREV_HASH),
                ),
                TemplateDistributionOwned::RequestTransactionDataSuccess(_) => {
                    Err(Self::Error::unexpected_message(
                        0,
                        MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
                    ))
                }
                TemplateDistributionOwned::RequestTransactionDataError(_) => Err(
                    Self::Error::unexpected_message(0, MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
                ),
            }
        }
    }

    async fn handle_coinbase_output_constraints(
        &mut self,
        client_id: Option<usize>,
        msg: CoinbaseOutputConstraintsOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionDataOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
    async fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolutionOwned,
        tlv_fields: Option<&[Tlv]>,
    ) -> Result<(), Self::Error>;
}
