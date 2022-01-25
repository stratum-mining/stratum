use super::SendTo_;
use crate::errors::Error;
use crate::parsers::TemplateDistribution;
use crate::utils::Mutex;
use template_distribution_sv2::{
    CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

pub type SendTo = SendTo_<TemplateDistribution<'static>, ()>;
use core::convert::TryInto;
use std::sync::Arc;

pub trait ParseServerTemplateDistributionMessages
where
    Self: Sized,
{
    fn handle_message_template_distribution(
        self_: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<SendTo, Error> {
        match (message_type, payload).try_into() {
            Ok(TemplateDistribution::NewTemplate(m)) => {
                self_.safe_lock(|x| x.handle_new_template(m)).unwrap()
            }
            Ok(TemplateDistribution::SetNewPrevHash(m)) => {
                self_.safe_lock(|x| x.handle_set_new_prev_hash(m)).unwrap()
            }
            Ok(TemplateDistribution::RequestTransactionDataSuccess(m)) => self_
                .safe_lock(|x| x.handle_request_tx_data_success(m))
                .unwrap(),
            Ok(TemplateDistribution::RequestTransactionDataError(m)) => self_
                .safe_lock(|x| x.handle_request_tx_data_error(m))
                .unwrap(),
            Ok(TemplateDistribution::CoinbaseOutputDataSize(_)) => Err(Error::UnexpectedMessage),
            Ok(TemplateDistribution::RequestTransactionData(_)) => Err(Error::UnexpectedMessage),
            Ok(TemplateDistribution::SubmitSolution(_)) => Err(Error::UnexpectedMessage),
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
        match (message_type, payload).try_into() {
            Ok(TemplateDistribution::CoinbaseOutputDataSize(m)) => self_
                .safe_lock(|x| x.handle_coinbase_out_data_size(m))
                .unwrap(),
            Ok(TemplateDistribution::RequestTransactionData(m)) => self_
                .safe_lock(|x| x.handle_request_tx_data(m))
                .unwrap(),
            Ok(TemplateDistribution::SubmitSolution(m)) => self_
                .safe_lock(|x| x.handle_request_submit_solution(m))
                .unwrap(),
            Ok(TemplateDistribution::NewTemplate(_)) => Err(Error::UnexpectedMessage),
            Ok(TemplateDistribution::SetNewPrevHash(_)) => Err(Error::UnexpectedMessage),
            Ok(TemplateDistribution::RequestTransactionDataSuccess(_)) => Err(Error::UnexpectedMessage),
            Ok(TemplateDistribution::RequestTransactionDataError(_)) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }
    fn handle_coinbase_out_data_size(&mut self, m: CoinbaseOutputDataSize)
        -> Result<SendTo, Error>;
    fn handle_request_tx_data(&mut self, m: RequestTransactionData) -> Result<SendTo, Error>;
    fn handle_request_submit_solution(&mut self, m: SubmitSolution) -> Result<SendTo, Error>;
}
