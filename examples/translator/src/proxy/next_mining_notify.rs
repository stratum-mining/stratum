use roles_logic_sv2::mining_sv2::{NewExtendedMiningJob, SetNewPrevHash};
use std::convert::TryInto;
use v1::{
    server_to_client,
    utils::{HexBytes, HexU32Be, PrevHash},
};

#[derive(Clone, Debug)]
pub struct NextMiningNotify {
    pub set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    pub new_extended_mining_job: Option<NewExtendedMiningJob<'static>>,
}

impl NextMiningNotify {
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

    pub(crate) fn create_notify(&self) -> Option<server_to_client::Notify> {
        // Put logic in to make sure that SetNewPrevHash + NewExtendedMiningJob is matching (not
        // future)
        // if new_prev_hash.job_id != new_job.job_id {
        //     panic!("TODO: SetNewPrevHash + NewExtendedMiningJob job id's do not match");
        // }

        if self.set_new_prev_hash.is_some() && self.new_extended_mining_job.is_some() {
            println!("\nRR CREATE_NOTIFY\n");
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
            let coin_base1 = HexBytes((&new_job.coinbase_tx_prefix).to_vec());
            let coin_base2 = HexBytes((&new_job.coinbase_tx_suffix).to_vec());

            // Seq0255<'static, U56<'static>> -> Vec<Vec<u8>> -> Vec<HexBytes>
            let merkle_path_seq0255 = &new_job.merkle_path;
            let merkle_path_vec = merkle_path_seq0255.clone().into_static();
            let merkle_path_vec: Vec<Vec<u8>> = merkle_path_vec.to_vec();
            let mut merkle_branch = Vec::<HexBytes>::new();
            for path in merkle_path_vec {
                // TODO: Check endianness
                merkle_branch.push(HexBytes(path).try_into().unwrap());
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
            Some(notify_response)
        } else {
            None
        }
    }
}
