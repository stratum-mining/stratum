//! Implements `ParseTemplateDistributionMessagesFromServer` for [`TemplateRx`].
//!
//! Handles incoming template distribution messages from the Template Provider and forwards them
//! as needed.
use super::TemplateRx;
use std::sync::Arc;
use stratum_common::roles_logic_sv2::{
    errors::Error,
    handlers::template_distribution::{ParseTemplateDistributionMessagesFromServer, SendTo},
    parsers_sv2::TemplateDistribution,
    template_distribution_sv2::*,
    utils::Mutex,
};
use tracing::{debug, error, info};

impl ParseTemplateDistributionMessagesFromServer for TemplateRx {
    // Handles a `NewTemplate` message and returns `RelayNewMessageToRemote`.
    fn handle_new_template(&mut self, m: NewTemplate) -> Result<SendTo, Error> {
        info!(
            "Received NewTemplate with id: {}, is future: {}",
            m.template_id, m.future_template
        );
        debug!("NewTemplate: {}", m);
        let new_template = TemplateDistribution::NewTemplate(m.into_static());
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            new_template,
        ))
    }

    // Handles a `SetNewPrevHash` and return `RelayNewMessageToRemote`
    fn handle_set_new_prev_hash(&mut self, m: SetNewPrevHash) -> Result<SendTo, Error> {
        info!("Received SetNewPrevHash for template: {}", m.template_id);
        debug!("SetNewPrevHash: {}", m);
        let new_prev_hash = TemplateDistribution::SetNewPrevHash(m.into_static());
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            new_prev_hash,
        ))
    }

    // Handles a `RequestTransactionDataSuccess` message and ignores it.
    //
    // This method is called when a `RequestTransactionDataSuccess` message is received.
    // This message is typically intended for Job Declarators, not the Template Receiver,
    // so it is logged and then ignored.
    fn handle_request_tx_data_success(
        &mut self,
        m: RequestTransactionDataSuccess,
    ) -> Result<SendTo, Error> {
        info!(
            "Received RequestTransactionDataSuccess for template: {}",
            m.template_id
        );
        debug!("RequestTransactionDataSuccess: {}", m);
        // Just ignore tx data messages this are meant for the declarators
        Ok(SendTo::None(None))
    }

    /// Handles a `RequestTransactionDataError` message and ignores it.
    ///
    /// This method is called when a `RequestTransactionDataError` message is received.
    /// This message is typically intended for Job Declarators, not the Template Receiver,
    /// so it is logged and then ignored.
    fn handle_request_tx_data_error(
        &mut self,
        m: RequestTransactionDataError,
    ) -> Result<SendTo, Error> {
        error!(
            "Received RequestTransactionDataError for template: {}, error: {}",
            m.template_id,
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        // Just ignore tx data messages this are meant for the declaretors
        Ok(SendTo::None(None))
    }
}
