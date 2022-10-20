<<<<<<< HEAD
use crate::ProxyResult;
use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
use std::convert::TryInto;
=======
use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
use v1::{
    server_to_client,
    utils::{HexBytes, HexU32Be, PrevHash},
};

<<<<<<< HEAD
/// Given a new SV2 `SetNewPrevHash` and/or `NewExtendedMiningJob` message, creates a new SV1
/// `mining.notify` message.
#[derive(Clone, Debug)]
pub struct NextMiningNotify {
    pub set_new_prev_hash: Option<SetNewPrevHash<'static>>,
=======
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
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
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

<<<<<<< HEAD
    /// Sets `set_new_prev_hash` member field upon `Bridge` receiving a SV2 `SetNewPrevHash`
=======
    /// Sets `set_new_prev_hash` member field upon the `Bridge` receiving a SV2 `SetNewPrevHash`
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    /// message from `Upstream`. Used in conjunction with `NewExtendedMiningJob` to create a SV1
    /// `mining.notify` message.
    pub(crate) fn set_new_prev_hash_msg(&mut self, set_new_prev_hash: SetNewPrevHash<'static>) {
        self.set_new_prev_hash = Some(set_new_prev_hash);
    }

<<<<<<< HEAD
    /// Sets `new_extended_mining_job` member field upon `Bridge` receiving a SV2
=======
    /// Sets `new_extended_mining_job` member field upon the `Bridge` receiving a SV2
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
    /// `NewExtendedMiningJob` message from `Upstream`. Used in conjunction with `SetNewPrevHash`
    /// to create a SV1 `mining.notify` message.
    pub(crate) fn new_extended_mining_job_msg(
        &mut self,
        new_extended_mining_job: NewExtendedMiningJob<'static>,
    ) {
        self.new_extended_mining_job = Some(new_extended_mining_job);
    }

<<<<<<< HEAD
    /// Creates a new SV1 `mining.notify` message on a new SV2 `SetNewPrevHash` and/or new
    /// `NewExtendedMiningJob` message.
    pub(crate) fn create_notify(&self) -> ProxyResult<Option<server_to_client::Notify>> {
        // Put logic in to make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not
        // future)
        // if new_prev_hash.job_id != new_job.job_id {
        //     panic!("TODO: SetNewPrevHash + NewExtendedMiningJob job id's do not match");
        // }

        if self.set_new_prev_hash.is_some() && self.new_extended_mining_job.is_some() {
            let new_prev_hash = match &self.set_new_prev_hash {
                Some(nph) => nph,
                None => panic!("Should never happen because of if statement"),
            };
            let new_job = match &self.new_extended_mining_job {
                Some(nj) => nj,
                None => panic!("Should never happen because of if statement"),
            };

            let job_id = new_prev_hash.job_id.to_string();

            // TODO: Check endianness
            // U256<'static> -> PrevHash
            let prev_hash = PrevHash((&new_prev_hash.prev_hash).to_vec());

            // TODO: Check endianness
            // B064K<'static'> -> HexBytes
            let coin_base1 = new_job.coinbase_tx_prefix.to_vec().try_into()?;
            let coin_base2 = new_job.coinbase_tx_suffix.to_vec().try_into()?;

            // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>> -> Vec<HexBytes>
            let merkle_path_seq0255 = &new_job.merkle_path;
            let merkle_path_vec = merkle_path_seq0255.clone().into_static();
            let merkle_path_vec: Vec<Vec<u8>> = merkle_path_vec.to_vec();
            let mut merkle_branch = Vec::<HexBytes>::new();
            for path in merkle_path_vec {
                // TODO: Check endianness
                merkle_branch.push(path.try_into()?);
            }

            // TODO: Check endianness
            // u32 -> HexBytes
            let version = HexU32Be(new_job.version);
            let bits = HexU32Be(new_prev_hash.nbits);
            let time = HexU32Be(new_prev_hash.min_ntime);

            let clean_jobs = false; // TODO: ?

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
            Ok(Some(notify_response))
        } else {
            Ok(None)
=======
    /// Creates a new SV1 `mining.notify` message if both SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages have been received. If one of these messages is still being
    /// waited on, the function returns `None`.
    pub(crate) fn create_notify(&self) -> Option<server_to_client::Notify> {
        // Make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not future)
        if self.set_new_prev_hash.is_some() && self.new_extended_mining_job.is_some() {
            let set_new_prev_hash_job_id = &self.set_new_prev_hash.as_ref().unwrap().job_id;
            let new_extended_mining_job_job_id =
                &self.new_extended_mining_job.as_ref().unwrap().job_id;
            if set_new_prev_hash_job_id != new_extended_mining_job_job_id {
                panic!(
                    "Job Id Mismatch. SetNewPrevHash Job Id: {}, NewExtendedMiningJob Job Id: {}",
                    set_new_prev_hash_job_id, new_extended_mining_job_job_id
                );
            }
        }

        match (&self.set_new_prev_hash, &self.new_extended_mining_job) {
            // If both the SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages are present,
            // translate into a SV1 `mining.notify` message
            (Some(new_prev_hash), Some(new_job)) => {
                let job_id = new_prev_hash.job_id.to_string();

                // U256<'static> -> PrevHash
                let prev_hash = new_prev_hash.prev_hash.clone().to_vec();
                let prev_hash = PrevHash(prev_hash);

                // B064K<'static'> -> HexBytes
                let coin_base1 = new_job.coinbase_tx_prefix.to_vec().into();
                let coin_base2 = new_job.coinbase_tx_suffix.to_vec().into();

                // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>> -> Vec<HexBytes>
                let merkle_path = new_job.merkle_path.clone().into_static().to_vec();
                let merkle_branch: Vec<HexBytes> =
                    merkle_path.into_iter().map(|p| p.into()).collect();

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
            }
            // If either of the SV2 `SetNewPrevHash` or `NewExtendedMiningJob` have not been
            // received, cannot make a SV1 `mining.notify` message so return `None`
            _ => None,
>>>>>>> e45f4adc1ef1104031c71ef3519f1cfaaff130c3
        }
    }
}
