use super::SendTo_;
use crate::{errors::Error, parsers::TemplateDistribution, utils::Mutex};
use template_distribution_sv2::{
    CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

pub type SendTo = SendTo_<TemplateDistribution<'static>, ()>;
use const_sv2::*;
use core::convert::TryInto;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

pub trait ParseServerTemplateDistributionMessages
where
    Self: Sized,
{
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_desrialized(
            self_,
            (message_type, payload).try_into(),
        )
    }
    fn handle_message_template_distribution_desrialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match message {
            Ok(TemplateDistribution::NewTemplate(m)) => {
                info!(
                    "Received NewTemplate with id: {}, is future: {}",
                    m.template_id, m.future_template
                );
                debug!("NewTemplate: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_new_template(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::SetNewPrevHash(m)) => {
                info!("Received SetNewPrevHash for template: {}", m.template_id);
                debug!("SetNewPrevHash: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_set_new_prev_hash(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                info!(
                    "Received RequestTransactionDataSuccess for template: {}",
                    m.template_id
                );
                trace!("RequestTransactionDataSuccess: {:?}", m);
                self_
                    .safe_lock(|x| x.handle_request_tx_data_success(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::RequestTransactionDataError(m)) => {
                error!(
                    "Received RequestTransactionDataError for template: {}, error: {}",
                    m.template_id,
                    std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
                );
                self_
                    .safe_lock(|x| x.handle_request_tx_data_error(m))
                    .map_err(|e| crate::Error::PoisonLock(e.to_string()))?
            }
            Ok(TemplateDistribution::CoinbaseOutputDataSize(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE,
            )),
            Ok(TemplateDistribution::RequestTransactionData(_)) => Err(Error::UnexpectedMessage(
                MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
            )),
            Ok(TemplateDistribution::SubmitSolution(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SUBMIT_SOLUTION))
            }
            Err(e) => Err(e),
        }
    }
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error>;
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error>;
    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error>;
    fn handle_request_tx_data_error(
        &mut self,
        m: RequestTransactionDataError,
    ) -> Result<SendTo, Error>;
}

pub trait ParseClientTemplateDistributionMessages
where
    Self: Sized,
{
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        Self::handle_message_template_distribution_desrialized(
            self_,
            (message_type, payload).try_into(),
        )
    }

    fn handle_message_template_distribution_desrialized(
        self_: Arc<Mutex<Self>>,
        message: Result<TemplateDistribution<'_>, Error>,
    ) -> Result<SendTo, Error> {
        // Is ok to unwrap a safe_lock result
        match message {
            Ok(TemplateDistribution::CoinbaseOutputDataSize(m)) => self_
                .safe_lock(|x| x.handle_coinbase_out_data_size(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::RequestTransactionData(m)) => self_
                .safe_lock(|x| x.handle_request_tx_data(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::SubmitSolution(m)) => self_
                .safe_lock(|x| x.handle_request_submit_solution(m))
                .map_err(|e| crate::Error::PoisonLock(e.to_string()))?,
            Ok(TemplateDistribution::NewTemplate(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_TEMPLATE))
            }
            Ok(TemplateDistribution::SetNewPrevHash(_)) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_NEW_PREV_HASH))
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(_)) => Err(
                Error::UnexpectedMessage(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS),
            ),
            Ok(TemplateDistribution::RequestTransactionDataError(_)) => Err(
                Error::UnexpectedMessage(MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR),
            ),
            Err(e) => Err(e),
        }
    }
    fn handle_coinbase_out_data_size(&mut self, m: CoinbaseOutputDataSize)
        -> Result<SendTo, Error>;
    fn handle_request_tx_data(&mut self, m: RequestTransactionData) -> Result<SendTo, Error>;
    fn handle_request_submit_solution(&mut self, m: SubmitSolution) -> Result<SendTo, Error>;
}
