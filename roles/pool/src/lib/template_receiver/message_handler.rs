use crate::template_receiver::TemplateRx;
use roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseServerTemplateDistributionMessages, SendTo},
    parsers::TemplateDistribution,
    template_distribution_sv2::*,
    utils::Mutex,
};
use std::sync::Arc;

impl ParseServerTemplateDistributionMessages for TemplateRx {
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error> {
        let new_template = TemplateDistribution::NewTemplate(m.into_static());
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            new_template,
        ))
    }

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error> {
        let new_prev_hash = TemplateDistribution::SetNewPrevHash(m.into_static());
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            new_prev_hash,
        ))
    }

    fn handle_request_tx_data_success(
        &mut self,
        _m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        // Just ignore tx data messages this are meant for the declaretors
        Ok(SendTo::None(None))
    }

    fn handle_request_tx_data_error(
        &mut self,
        _m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        // Just ignore tx data messages this are meant for the declaretors
        Ok(SendTo::None(None))
    }
}
