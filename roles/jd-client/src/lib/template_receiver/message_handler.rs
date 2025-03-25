use super::TemplateRx;
use roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseTemplateDistributionMessagesFromServer, SendTo},
    parsers::TemplateDistribution,
    template_distribution_sv2::*,
};
use tracing::{debug, error, info};

impl ParseTemplateDistributionMessagesFromServer for TemplateRx {
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error> {
        info!(
            "Received NewTemplate with id: {}, is future: {}",
            m.template_id, m.future_template
        );
        debug!("NewTemplate: {:?}", m);
        let new_template = m.into_static();
        let new_template = TemplateDistribution::NewTemplate(new_template);
        Ok(SendTo::None(Some(new_template)))
    }

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error> {
        info!("Received SetNewPrevHash for template: {}", m.template_id);
        debug!("SetNewPrevHash: {:?}", m);
        let new_prev_hash = SetNewPrevHash {
            template_id: m.template_id,
            prev_hash: m.prev_hash.into_static(),
            header_timestamp: m.header_timestamp,
            n_bits: m.n_bits,
            target: m.target.into_static(),
        };
        let new_prev_hash = TemplateDistribution::SetNewPrevHash(new_prev_hash);
        self.pool_chaneger_trigger.safe_lock(|t| t.stop()).unwrap();
        Ok(SendTo::None(Some(new_prev_hash)))
    }

    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received RequestTransactionDataSuccess for template: {}",
            m.template_id
        );
        debug!("RequestTransactionDataSuccess: {:?}", m);
        let m = RequestTransactionDataSuccess {
            transaction_list: m.transaction_list.into_static(),
            excess_data: m.excess_data.into_static(),
            template_id: m.template_id,
        };
        let tx_received = TemplateDistribution::RequestTransactionDataSuccess(m);
        Ok(SendTo::None(Some(tx_received)))
    }

    fn handle_request_tx_data_error(
        &mut self,
        m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        error!(
            "Received RequestTransactionDataError for template: {}, error: {}",
            m.template_id,
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        let m = RequestTransactionDataError {
            template_id: m.template_id,
            error_code: m.error_code.into_static(),
        };
        let error_code_string =
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code");
        match error_code_string {
            "template-id-not-found" => Err(Error::NoValidTemplate(error_code_string.to_string())),
            "stale-template-id" => Ok(SendTo::None(Some(
                TemplateDistribution::RequestTransactionDataError(m),
            ))),
            _ => Err(Error::NoValidTemplate(error_code_string.to_string())),
        }
    }
}
