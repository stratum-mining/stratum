use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
use tracing::{debug, error};
use v1::{
    server_to_client,
    utils::{HexU32Be, MerkleNode, PrevHash},
};

/// To create a SV1 `mining.notify` message, both a SV2 `SetNewPrevHash` and `NewExtendedMiningJob`
/// message must be received. These messages are sent by the Upstream role at different times, so
/// this struct serves as the means to store each message as they are received. It also performs
/// the translation into the SV1 `mining.notify` message.
#[derive(Clone, Debug)]
pub struct NextMiningNotify {
    /// Stores the SV2 `SetNewPrevHash` message to be later translated (along with the SV2
    /// `NewExtendedMiningJob` message) to a SV1 `mining.notify` message.
    pub set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    /// Stores the SV2 `NewExtendedMiningJob` message to be later translated (along with the SV2
    /// `SetNewPrevHash` message) to a SV1 `mining.notify` message.
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

    /// Sets `set_new_prev_hash` member field upon the `Bridge` receiving a SV2 `SetNewPrevHash`
    /// message from `Upstream`. Used in conjunction with `NewExtendedMiningJob` to create a SV1
    /// `mining.notify` message.
    pub(crate) fn set_new_prev_hash_msg(&mut self, set_new_prev_hash: SetNewPrevHash<'static>) {
        self.set_new_prev_hash = Some(set_new_prev_hash);
    }

    /// Sets `new_extended_mining_job` member field upon the `Bridge` receiving a SV2
    /// `NewExtendedMiningJob` message from `Upstream`. Used in conjunction with `SetNewPrevHash`
    /// to create a SV1 `mining.notify` message.
    pub(crate) fn new_extended_mining_job_msg(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJob<'static>,
    ) {
        self.new_extended_mining_job = Some(new_extended_mining_job);
    }

    /// Checks that `SetNewPrevHash` and `NewExtendedMiningJob` stored in `NextMiningNotify` are
    /// for the same job (aka each message has the same `job_id` field).
    pub(crate) fn matching_job_id(&self) -> bool {
        if let (Some(snph), Some(nemj)) = (
            self.set_new_prev_hash.as_ref(),
            self.new_extended_mining_job.as_ref(),
        ) {
            snph.job_id.eq(&nemj.job_id)
        } else {
            false
        }
    }

    /// Creates a new SV1 `mining.notify` message if both SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages have been received. If one of these messages is still being
    /// waited on, the function returns `None`.
    pub(crate) fn create_notify(&self) -> Option<server_to_client::Notify<'static>> {
        // Make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not future)
        if !self.matching_job_id() {
            let (snph_job_id, nemj_job_id) = match (
                self.set_new_prev_hash.as_ref(),
                self.new_extended_mining_job.as_ref(),
            ) {
                (Some(snph), Some(nemj)) => (Some(snph.job_id), Some(nemj.job_id)),
                (Some(snph), None) => (Some(snph.job_id), None),
                (None, Some(nemj)) => (None, Some(nemj.job_id)),
                (None, None) => (None, None),
            };
            error!(
                "Job Id Mismatch. SetNewPrevHash Job Id: {:?}, NewExtendedMiningJob Job Id: {:?}",
                snph_job_id, nemj_job_id
            );
            return None;
        }

        match (&self.set_new_prev_hash, &self.new_extended_mining_job) {
            // If both the SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages are present,
            // translate into a SV1 `mining.notify` message
            (Some(new_prev_hash), Some(new_job)) => {
                let job_id = new_prev_hash.job_id.to_string();

                // U256<'static> -> MerkleLeaf
                let prev_hash = PrevHash(new_prev_hash.prev_hash.clone());

                // B064K<'static'> -> HexBytes
                let coin_base1 = new_job.coinbase_tx_prefix.to_vec().into();
                let coin_base2 = new_job.coinbase_tx_suffix.to_vec().into();

                // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>>
                let merkle_path = new_job.merkle_path.clone().into_static().0;
                let merkle_branch: Vec<MerkleNode> =
                    merkle_path.into_iter().map(MerkleNode).collect();

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
                debug!("\nNextMiningNotify: {:?}\n", &self);
                Some(notify_response)
            }
            // If either of the SV2 `SetNewPrevHash` or `NewExtendedMiningJob` have not been
            // received, cannot make a SV1 `mining.notify` message so return `None`
            _ => None,
        }
    }
}
