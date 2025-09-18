use parsers_sv2::TemplateDistribution;
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

use core::convert::TryInto;
use template_distribution_sv2::*;

use crate::error::HandlerErrorType;

pub trait HandleTemplateDistributionMessagesFromServerSync<Error: HandlerErrorType> {
    type Error;

    fn handle_template_distribution_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: TemplateDistribution<'_> = (message_type, payload)
            .try_into()
            .map_err(Error::parse_error)?;
        self.handle_template_distribution_message_from_server(parsed)
    }

    fn handle_template_distribution_message_from_server(
        &mut self,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Error> {
        match message {
            TemplateDistribution::NewTemplate(m) => self.handle_new_template(m),
            TemplateDistribution::SetNewPrevHash(m) => self.handle_set_new_prev_hash(m),
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                self.handle_request_tx_data_success(m)
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                self.handle_request_tx_data_error(m)
            }

            TemplateDistribution::CoinbaseOutputConstraints(_) => Err(Error::unexpected_message(
                MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
            )),
            TemplateDistribution::RequestTransactionData(_) => Err(Error::unexpected_message(
                MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
            )),
            TemplateDistribution::SubmitSolution(_) => {
                Err(Error::unexpected_message(MESSAGE_TYPE_SUBMIT_SOLUTION))
            }
        }
    }
    fn handle_new_template(&mut self, msg: NewTemplate) -> Result<(), Error>;

    fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) -> Result<(), Error>;

    fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess,
    ) -> Result<(), Error>;

    fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError,
    ) -> Result<(), Error>;
}

#[trait_variant::make(Send)]
pub trait HandleTemplateDistributionMessagesFromServerAsync<Error: HandlerErrorType> {
    type Error;

    async fn handle_template_distribution_message_frame_from_server(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        async move {
            let parsed: TemplateDistribution<'_> = (message_type, payload)
                .try_into()
                .map_err(Error::parse_error)?;
            self.handle_template_distribution_message_from_server(parsed)
                .await
        }
    }

    async fn handle_template_distribution_message_from_server(
        &mut self,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Error> {
        async move {
            match message {
                TemplateDistribution::NewTemplate(m) => self.handle_new_template(m).await,
                TemplateDistribution::SetNewPrevHash(m) => self.handle_set_new_prev_hash(m).await,
                TemplateDistribution::RequestTransactionDataSuccess(m) => {
                    self.handle_request_tx_data_success(m).await
                }
                TemplateDistribution::RequestTransactionDataError(m) => {
                    self.handle_request_tx_data_error(m).await
                }

                TemplateDistribution::CoinbaseOutputConstraints(_) => Err(
                    Error::unexpected_message(MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS),
                ),
                TemplateDistribution::RequestTransactionData(_) => Err(Error::unexpected_message(
                    MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
                )),
                TemplateDistribution::SubmitSolution(_) => {
                    Err(Error::unexpected_message(MESSAGE_TYPE_SUBMIT_SOLUTION))
                }
            }
        }
    }
    async fn handle_new_template(&mut self, msg: NewTemplate) -> Result<(), Error>;

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) -> Result<(), Error>;

    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess,
    ) -> Result<(), Error>;

    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError,
    ) -> Result<(), Error>;
}

pub trait HandleTemplateDistributionMessagesFromClientSync<Error: HandlerErrorType> {
    type Error;

    fn handle_template_distribution_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        let parsed: TemplateDistribution<'_> = (message_type, payload)
            .try_into()
            .map_err(Error::parse_error)?;
        self.handle_template_distribution_message_from_client(parsed)
    }

    fn handle_template_distribution_message_from_client(
        &mut self,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Error> {
        match message {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                self.handle_coinbase_output_constraints(m)
            }
            TemplateDistribution::RequestTransactionData(m) => self.handle_request_tx_data(m),
            TemplateDistribution::SubmitSolution(m) => self.handle_submit_solution(m),

            TemplateDistribution::NewTemplate(_) => {
                Err(Error::unexpected_message(MESSAGE_TYPE_NEW_TEMPLATE))
            }
            TemplateDistribution::SetNewPrevHash(_) => {
                Err(Error::unexpected_message(MESSAGE_TYPE_SET_NEW_PREV_HASH))
            }
            TemplateDistribution::RequestTransactionDataSuccess(_) => Err(
                Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            TemplateDistribution::RequestTransactionDataError(_) => Err(Error::unexpected_message(
                MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
            )),
        }
    }

    fn handle_coinbase_output_constraints(
        &mut self,
        msg: CoinbaseOutputConstraints,
    ) -> Result<(), Error>;

    fn handle_request_tx_data(&mut self, msg: RequestTransactionData) -> Result<(), Error>;
    fn handle_submit_solution(&mut self, msg: SubmitSolution) -> Result<(), Error>;
}

#[trait_variant::make(Send)]
pub trait HandleTemplateDistributionMessagesFromClientAsync<Error: HandlerErrorType> {
    type Error;

    async fn handle_template_distribution_message_frame_from_client(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<(), Error> {
        async move {
            let parsed: TemplateDistribution<'_> = (message_type, payload)
                .try_into()
                .map_err(Error::parse_error)?;
            self.handle_template_distribution_message_from_client(parsed)
                .await
        }
    }

    async fn handle_template_distribution_message_from_client(
        &mut self,
        message: TemplateDistribution<'_>,
    ) -> Result<(), Error> {
        async move {
            match message {
                TemplateDistribution::CoinbaseOutputConstraints(m) => {
                    self.handle_coinbase_output_constraints(m).await
                }
                TemplateDistribution::RequestTransactionData(m) => {
                    self.handle_request_tx_data(m).await
                }
                TemplateDistribution::SubmitSolution(m) => self.handle_submit_solution(m).await,

                TemplateDistribution::NewTemplate(_) => {
                    Err(Error::unexpected_message(MESSAGE_TYPE_NEW_TEMPLATE))
                }
                TemplateDistribution::SetNewPrevHash(_) => {
                    Err(Error::unexpected_message(MESSAGE_TYPE_SET_NEW_PREV_HASH))
                }
                TemplateDistribution::RequestTransactionDataSuccess(_) => Err(
                    Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
                ),
                TemplateDistribution::RequestTransactionDataError(_) => Err(
                    Error::unexpected_message(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
                ),
            }
        }
    }

    async fn handle_coinbase_output_constraints(
        &mut self,
        msg: CoinbaseOutputConstraints,
    ) -> Result<(), Error>;

    async fn handle_request_tx_data(&mut self, msg: RequestTransactionData) -> Result<(), Error>;
    async fn handle_submit_solution(&mut self, msg: SubmitSolution) -> Result<(), Error>;
}
