use crate::utils::{Id, Mutex};
use bitcoin_hashes::{sha256d, Hash, HashEngine};
use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, SetNewPrevHash, SubmitSharesError, SubmitSharesStandard,
    Target,
};
//use crate::common_properties::StandardChannel;
use crate::common_properties::StandardChannel;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

fn extended_to_standard_job_for_group_channel<'a>(
    extended: &NewExtendedMiningJob,
    extranonce: &[u8],
    channel_id: u32,
    job_id: u32,
) -> NewMiningJob<'a> {
    let merkle_root = merkle_root_from_path(
        extended.coinbase_tx_prefix.inner_as_ref(),
        extended.coinbase_tx_suffix.inner_as_ref(),
        extranonce,
        &extended.merkle_path.inner_as_ref(),
    );
    NewMiningJob {
        channel_id,
        job_id,
        future_job: extended.future_job,
        version: extended.version,
        merkle_root: merkle_root.try_into().unwrap(),
    }
}

fn merkle_root_from_path(
    coinbase_tx_prefix: &[u8],
    coinbase_tx_suffix: &[u8],
    extranonce: &[u8],
    path: &[&[u8]],
) -> Vec<u8> {
    let mut coinbase =
        Vec::with_capacity(coinbase_tx_prefix.len() + coinbase_tx_suffix.len() + extranonce.len());
    coinbase.extend_from_slice(coinbase_tx_prefix);
    coinbase.extend_from_slice(extranonce);
    coinbase.extend_from_slice(coinbase_tx_suffix);

    let mut engine = sha256d::Hash::engine();
    engine.input(&coinbase);
    let coinbase = sha256d::Hash::from_engine(engine);

    let root = path.iter().fold(coinbase, |root, leaf| {
        let mut engine = sha256d::Hash::engine();
        engine.input(&root);
        engine.input(leaf);
        sha256d::Hash::from_engine(engine)
    });
    root.to_vec()
}

#[allow(dead_code)]
struct BlockHeader<'a> {
    version: u32,
    prev_hash: &'a [u8],
    merkle_root: &'a [u8],
    timestamp: u32,
    nbits: u32,
    nonce: u32,
}

impl<'a> BlockHeader<'a> {
    #[allow(dead_code)]
    pub fn hash(&self) -> Target {
        let mut engine = sha256d::Hash::engine();
        engine.input(&self.version.to_le_bytes());
        engine.input(&self.prev_hash);
        engine.input(&self.merkle_root);
        engine.input(&self.timestamp.to_be_bytes());
        engine.input(&self.nbits.to_be_bytes());
        engine.input(&self.nonce.to_be_bytes());
        let hashed = sha256d::Hash::from_engine(engine).into_inner();
        hashed.into()
    }
}

#[allow(dead_code)]
fn target_from_shares(
    job: &DownstreamJob,
    prev_hash: &[u8],
    nbits: u32,
    share: &SubmitSharesStandard,
) -> Target {
    let header = BlockHeader {
        version: share.version,
        prev_hash,
        merkle_root: &job.merkle_root,
        timestamp: share.ntime,
        nbits,
        nonce: share.nonce,
    };
    header.hash()
}

//#[derive(Debug)]
//pub struct StandardChannel {
//    target: Target,
//    extranonce: Extranonce,
//    id: u32,
//}

#[derive(Debug)]
struct DownstreamJob {
    merkle_root: Vec<u8>,
    extended_job_id: u32,
}

#[derive(Debug)]
struct ExtendedJobs {
    upstream_target: Vec<u8>,
}

#[derive(Debug)]
pub struct GourpChannelJobDispatcher {
    //channels: Vec<StandardChannel>,
    target: Target,
    prev_hash: Vec<u8>,
    // extedned_job_id -> standard_job_id -> standard_job
    future_jobs: HashMap<u32, HashMap<u32, DownstreamJob>>,
    // standard_job_id -> standard_job
    jobs: HashMap<u32, DownstreamJob>,
    ids: Arc<Mutex<Id>>,
    nbits: u32,
}

pub enum SendSharesResponse {
    //ValidAndMeetUpstreamTarget((SubmitSharesStandard,SubmitSharesSuccess)),
    Valid(SubmitSharesStandard),
    Invalid(SubmitSharesError<'static>),
}

impl GourpChannelJobDispatcher {
    pub fn new(ids: Arc<Mutex<Id>>) -> Self {
        Self {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::new(),
            jobs: HashMap::new(),
            ids,
            nbits: 0,
        }
    }

    pub fn on_new_extended_mining_job(
        &mut self,
        extended: &NewExtendedMiningJob,
        channel: &StandardChannel,
    ) -> NewMiningJob<'static> {
        if extended.future_job {
            self.future_jobs.insert(extended.job_id, HashMap::new());
        };
        let extranonce: Vec<u8> = channel.extranonce.clone().into();
        let new_mining_job_message = extended_to_standard_job_for_group_channel(
            &extended,
            &extranonce,
            channel.channel_id,
            self.ids.safe_lock(|ids| ids.next()).unwrap(),
        );
        let job = DownstreamJob {
            merkle_root: new_mining_job_message.merkle_root.to_vec(),
            extended_job_id: extended.job_id,
        };
        if extended.future_job {
            let future_jobs = self.future_jobs.get_mut(&extended.job_id).unwrap();
            future_jobs.insert(new_mining_job_message.job_id, job);
        } else {
            self.jobs.insert(new_mining_job_message.job_id, job);
        };
        new_mining_job_message
    }

    pub fn on_new_prev_hash(&mut self, message: &SetNewPrevHash) {
        let jobs = self.future_jobs.get_mut(&message.job_id).unwrap();
        std::mem::swap(&mut self.jobs, jobs);
        self.prev_hash = message.prev_hash.to_vec();
        self.nbits = message.nbits;
        self.future_jobs.clear();
    }

    // (response, upstream id)
    pub fn on_submit_shares(&self, shares: SubmitSharesStandard) -> SendSharesResponse {
        let id = shares.job_id;
        if let Some(job) = self.jobs.get(&id) {
            //let target = target_from_shares(
            //    job,
            //    &self.prev_hash,
            //    self.nbits,
            //    &shares,
            //    );
            //match target >= self.target {
            //    true => SendSharesResponse::ValidAndMeetUpstreamTarget(success),
            //    false => SendSharesResponse::Valid(success),
            //}
            let success = SubmitSharesStandard {
                channel_id: shares.channel_id,
                sequence_number: shares.sequence_number,
                job_id: job.extended_job_id,
                nonce: shares.nonce,
                ntime: shares.ntime,
                version: shares.version,
            };
            SendSharesResponse::Valid(success)
        } else {
            let error = SubmitSharesError {
                channel_id: shares.channel_id,
                sequence_number: shares.sequence_number,
                error_code: "".to_string().into_bytes().try_into().unwrap(),
            };
            SendSharesResponse::Invalid(error)
        }
    }
}
