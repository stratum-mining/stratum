use stratum_common::roles_logic_sv2::{
    bitcoin::{Amount, Transaction, TxOut, consensus, hashes::Hash},
    channels_sv2::chain_tip::ChainTip,
    codec_sv2::binary_sv2::{Seq064K, U256},
    handlers_sv2::HandleTemplateDistributionMessagesFromServerAsync,
    job_declaration_sv2::DeclareMiningJob,
    mining_sv2::SetNewPrevHash as SetNewPrevHashMp,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::*,
};
use tracing::{error, info, warn};

use crate::{
    channel_manager::ChannelManager,
    error::PoolError,
    utils::{StdFrame, deserialize_coinbase_outputs},
};

impl HandleTemplateDistributionMessagesFromServerAsync for ChannelManager {
    type Error = PoolError;

    async fn handle_new_template(&mut self, msg: NewTemplate<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Ok(())
    }

    async fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }

    async fn handle_set_new_prev_hash(
        &mut self,
        msg: SetNewPrevHash<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        Ok(())
    }
}
