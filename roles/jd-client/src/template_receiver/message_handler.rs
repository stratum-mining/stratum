use crate::template_receiver::TemplateRx;
use roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseServerTemplateDistributionMessages, SendTo},
    parsers::TemplateDistribution,
    template_distribution_sv2::*,
};

impl ParseServerTemplateDistributionMessages for TemplateRx {
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error> {
        let new_template = m.into_static();
        let new_template = TemplateDistribution::NewTemplate(new_template);
        Ok(SendTo::None(Some(new_template)))
    }

    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error> {
        let new_prev_hash = SetNewPrevHash {
            template_id: m.template_id,
            prev_hash: m.prev_hash.into_static(),
            header_timestamp: m.header_timestamp,
            n_bits: m.n_bits,
            target: m.target.into_static(),
        };
        let new_prev_hash = TemplateDistribution::SetNewPrevHash(new_prev_hash);
        Ok(SendTo::None(Some(new_prev_hash)))
    }

    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        self.transactions_data = m.transaction_list.into_static();
        self.excess_data = m.excess_data.into_static();
        Ok(SendTo::None(None))
    }

    fn handle_request_tx_data_error(
        &mut self,
        _m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        todo!()
    }
}
