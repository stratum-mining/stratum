use crate::ProxyResult;
use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
use std::convert::TryInto;
use v1::{
    server_to_client,
    utils::{HexBytes, HexU32Be, PrevHash},
};

/// Given a new SV2 `SetNewPrevHash` and/or `NewExtendedMiningJob` message, creates a new SV1
/// `mining.notify` message.
#[derive(Clone, Debug)]
pub struct NextMiningNotify {
    pub set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    pub new_extended_mining_job: Option<NewExtendedMiningJob<'static>>,
}

impl NextMiningNotify {
    /// Instantiates a new `NextMiningNotify`.
    pub(crate) fn new() -> Self {
        NextMiningNotify {
            set_new_prev_hash: None,
            new_extended_mining_job: None,
        }
    }

    /// Sets `set_new_prev_hash` member field upon `Bridge` receiving a SV2 `SetNewPrevHash`
    /// message from `Upstream`. Used in conjunction with `NewExtendedMiningJob` to create a SV1
    /// `mining.notify` message.
    pub(crate) fn set_new_prev_hash_msg(&mut self, set_new_prev_hash: SetNewPrevHash<'static>) {
        self.set_new_prev_hash = Some(set_new_prev_hash);
    }

    /// Sets `new_extended_mining_job` member field upon `Bridge` receiving a SV2
    /// `NewExtendedMiningJob` message from `Upstream`. Used in conjunction with `SetNewPrevHash`
    /// to create a SV1 `mining.notify` message.
    pub(crate) fn new_extended_mining_job_msg(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJob<'static>,
    ) {
        self.new_extended_mining_job = Some(new_extended_mining_job);
    }

    /// Creates a new SV1 `mining.notify` message on a new SV2 `SetNewPrevHash` and/or new
    /// `NewExtendedMiningJob` message.
    pub(crate) fn create_notify(&self) -> Option<server_to_client::Notify> {
        // Put logic in to make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not
        // future)
        // if new_prev_hash.job_id != new_job.job_id {
        //     panic!("TODO: SetNewPrevHash + NewExtendedMiningJob job id's do not match");
        // }
        //
        match (&self.set_new_prev_hash, &self.new_extended_mining_job) {
            (Some(new_prev_hash),Some(new_job)) => {
                let job_id = new_prev_hash.job_id.to_string();

                // TODO: Check endianness
                // U256<'static> -> PrevHash
                let mut prev_hash = new_prev_hash.prev_hash.clone().to_vec();
                let prev_hash = PrevHash(prev_hash);

                // TODO: Check endianness
                // B064K<'static'> -> HexBytes
                let coin_base1 = new_job.coinbase_tx_prefix.to_vec().into();
                let coin_base2 = new_job.coinbase_tx_suffix.to_vec().into();

                // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>> -> Vec<HexBytes>
                let merkle_path = new_job.merkle_path.clone().into_static().to_vec();
                let merkle_branch: Vec<HexBytes> = merkle_path.into_iter().map(|p| p.into()).collect();

                // TODO: Check endianness
                // u32 -> HexBytes
                let version = HexU32Be(new_job.version);
                let bits = HexU32Be(new_prev_hash.nbits);
                let time = HexU32Be(new_prev_hash.min_ntime);

                let clean_jobs = true;

                let notify_response = server_to_client::Notify {
                    job_id,
                    prev_hash,
                    coin_base1,
                    coin_base2,
                    merkle_branch,
                    version,
                    bits,
                    time,
                    clean_jobs,
                };
                Some(notify_response)
            },
            _ => None,
        }
    }
}
