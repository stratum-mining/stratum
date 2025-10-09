use parsers_sv2::TemplateDistribution;
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

use core::convert::TryInto;
use template_distribution_sv2::*;

use crate::error::HandlerErrorType;

/// Synchronous handler trait for processing template distribution messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
pub trait HandleTemplateDistributionMessagesFromServerSync {
    type Error: HandlerErrorType;

    fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: TemplateDistribution<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_template_distribution_message_from_server(server_id, parsed)
    }

    fn handle_template_distribution_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistribution::NewTemplate(m) => self.handle_new_template(server_id, m),
            TemplateDistribution::SetNewPrevHash(m) => self.handle_set_new_prev_hash(server_id, m),
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                self.handle_request_tx_data_success(server_id, m)
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                self.handle_request_tx_data_error(server_id, m)
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
    ) -> Result<(), Self::Error>;

    fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccess,
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataError,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing template distribution messages received from servers.
///
/// The server ID identifies which server a message originated from.
/// It is optional because when there is only a single server,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleTemplateDistributionMessagesFromServerAsync {
    type Error: HandlerErrorType;

    async fn handle_template_distribution_message_frame_from_server(
        &mut self,
        server_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: TemplateDistribution<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_template_distribution_message_from_server(server_id, parsed)
                .await
        }
    }

    async fn handle_template_distribution_message_from_server(
        &mut self,
        server_id: Option<usize>,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistribution::NewTemplate(m) => {
                    self.handle_new_template(server_id, m).await
                }
                TemplateDistribution::SetNewPrevHash(m) => {
                    self.handle_set_new_prev_hash(server_id, m).await
                }
                TemplateDistribution::RequestTransactionDataSuccess(m) => {
                    self.handle_request_tx_data_success(server_id, m).await
                }
                TemplateDistribution::RequestTransactionDataError(m) => {
                    self.handle_request_tx_data_error(server_id, m).await
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
    ) -> Result<(), Self::Error>;

    async fn handle_set_new_prev_hash(
        &mut self,
        server_id: Option<usize>,
        msg: SetNewPrevHash,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_success(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataSuccess,
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data_error(
        &mut self,
        server_id: Option<usize>,
        msg: RequestTransactionDataError,
    ) -> Result<(), Self::Error>;
}

/// Synchronous handler trait for processing template distribution messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
pub trait HandleTemplateDistributionMessagesFromClientSync {
    type Error: HandlerErrorType;

    fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        let parsed: TemplateDistribution<'_> = (message_type, payload)
            .try_into()
            .map_err(Self::Error::parse_error)?;
        self.handle_template_distribution_message_from_client(client_id, parsed)
    }

    fn handle_template_distribution_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Self::Error> {
        match message {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                self.handle_coinbase_output_constraints(client_id, m)
            }
            TemplateDistribution::RequestTransactionData(m) => {
                self.handle_request_tx_data(client_id, m)
            }
            TemplateDistribution::SubmitSolution(m) => self.handle_submit_solution(client_id, m),

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
    ) -> Result<(), Self::Error>;

    fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionData,
    ) -> Result<(), Self::Error>;
    fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolution,
    ) -> Result<(), Self::Error>;
}

/// Asynchronous handler trait for processing template distribution messages received from clients.
///
/// The client ID identifies which client a message originated from.
/// It is optional because when there is only a single client,
/// an explicit ID is unnecessary.
#[trait_variant::make(Send)]
pub trait HandleTemplateDistributionMessagesFromClientAsync {
    type Error: HandlerErrorType;

    async fn handle_template_distribution_message_frame_from_client(
        &mut self,
        client_id: Option<usize>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Self::Error> {
        async move {
            let parsed: TemplateDistribution<'_> = (message_type, payload)
                .try_into()
                .map_err(Self::Error::parse_error)?;
            self.handle_template_distribution_message_from_client(client_id, parsed)
                .await
        }
    }

    async fn handle_template_distribution_message_from_client(
        &mut self,
        client_id: Option<usize>,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Self::Error> {
        async move {
            match message {
                TemplateDistribution::CoinbaseOutputConstraints(m) => {
                    self.handle_coinbase_output_constraints(client_id, m).await
                }
                TemplateDistribution::RequestTransactionData(m) => {
                    self.handle_request_tx_data(client_id, m).await
                }
                TemplateDistribution::SubmitSolution(m) => {
                    self.handle_submit_solution(client_id, m).await
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
    ) -> Result<(), Self::Error>;

    async fn handle_request_tx_data(
        &mut self,
        client_id: Option<usize>,
        msg: RequestTransactionData,
    ) -> Result<(), Self::Error>;
    async fn handle_submit_solution(
        &mut self,
        client_id: Option<usize>,
        msg: SubmitSolution,
    ) -> Result<(), Self::Error>;
}
