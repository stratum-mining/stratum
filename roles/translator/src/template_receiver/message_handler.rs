use crate::template_receiver::TemplateRx;
use roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseServerTemplateDistributionMessages, SendTo},
    parsers::TemplateDistribution,
    template_distribution_sv2::*,
};

impl ParseServerTemplateDistributionMessages for TemplateRx {
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error> {
        let new_template = NewTemplate {
            template_id: m.template_id,
            future_template: m.future_template,
            version: m.version,
            coinbase_tx_version: m.coinbase_tx_version,
            coinbase_prefix: m.coinbase_prefix.into_static(),
            coinbase_tx_input_sequence: m.coinbase_tx_input_sequence,
            coinbase_tx_value_remaining: m.coinbase_tx_value_remaining,
            coinbase_tx_outputs_count: m.coinbase_tx_outputs_count,
            coinbase_tx_outputs: m.coinbase_tx_outputs.into_static(),
            coinbase_tx_locktime: m.coinbase_tx_locktime,
            merkle_path: m.merkle_path.into_static(),
        };
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
        _m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        todo!()
    }

    fn handle_request_tx_data_error(
        &mut self,
        _m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        todo!()
    }
}
