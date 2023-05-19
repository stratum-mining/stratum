use crate::lib::template_receiver::TemplateRx;
use roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseServerTemplateDistributionMessages, SendTo},
    template_distribution_sv2::*,
};

impl ParseServerTemplateDistributionMessages for TemplateRx {
    fn handle_new_template(&mut self, _: NewTemplate) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_set_new_prev_hash(&mut self, _: SetNewPrevHash) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_request_tx_data_success(
        &mut self,
        _m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }

    fn handle_request_tx_data_error(
        &mut self,
        _m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        Ok(SendTo::None(None))
    }
}
