use crate::{
    errors::Error,
    utils::{Id, Mutex},
};
use bitcoin::hashes::{sha256d, Hash, HashEngine};
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
pub struct GroupChannelJobDispatcher {
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

impl GroupChannelJobDispatcher {
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

    pub fn on_new_prev_hash(&mut self, message: &SetNewPrevHash) -> Result<(), Error> {
        if self.future_jobs.is_empty() {
            return Err(Error::NoFutureJobs);
        }
        let jobs = match self.future_jobs.get_mut(&message.job_id) {
            Some(j) => j,
            // TODO: What error would exist here? Is there a scenario where a value of
            // message.job_id would cause an error?
            _ => panic!("TODO: What is the appropriate error here?"),
        };
        std::mem::swap(&mut self.jobs, jobs);
        self.prev_hash = message.prev_hash.to_vec();
        self.nbits = message.nbits;
        self.future_jobs.clear();
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use binary_sv2::{u256_from_int, Seq0255, B064K, U256};

    #[test]
    fn builds_group_channel_job_dispatcher() {
        let expect = GroupChannelJobDispatcher {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::new(),
            jobs: HashMap::new(),
            ids: Arc::new(Mutex::new(Id::new())),
            nbits: 0,
        };

        let ids = Arc::new(Mutex::new(Id::new()));
        let actual = GroupChannelJobDispatcher::new(ids);

        assert_eq!(expect.target, actual.target);
        assert_eq!(expect.prev_hash, actual.prev_hash);
        assert_eq!(expect.nbits, actual.nbits);
        assert!(actual.future_jobs.is_empty());
        assert!(actual.jobs.is_empty());
        // TODO: check actual.ids, but idk how to properly test arc
        // assert_eq!(expect.ids, actual.ids);
    }

    #[test]
    fn updates_group_channel_job_dispatcher_on_new_extended_mining_job() {
        let channel_id = 0;
        let coinbase_tx_prefix: B064K = vec![0x54, 0x03, 0x4f, 0x06, 0x0b].try_into().unwrap();
        let coinbase_tx_suffix: B064K = vec![
            0x1b, 0x4d, 0x69, 0x6e, 0x65, 0x64, 0x20, 0x62, 0x79, 0x20, 0x41, 0x6e, 0x74, 0x50,
            0x6f, 0x6f, 0x6c, 0x37, 0x34, 0x32, 0x50, 0x00, 0xb5, 0x03, 0x65, 0xad, 0x84, 0xd3,
            0xfa, 0xbe, 0x6d, 0x6d, 0x8a, 0xa3, 0x76, 0x66, 0x5f, 0x34, 0xd5, 0xc9, 0x70, 0x1b,
            0xd9, 0x61, 0x6d, 0xae, 0x1f, 0x69, 0x98, 0x2b, 0x75, 0x78, 0x01, 0x45, 0xde, 0x2e,
            0x30, 0xc1, 0xbf, 0xf3, 0xd5, 0x29, 0x08, 0x3c, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xc1, 0xb6, 0x22, 0x00, 0x15, 0x2e, 0x00, 0x00,
        ]
        .try_into()
        .unwrap();

        let extended = NewExtendedMiningJob {
            channel_id,
            job_id: 0,
            future_job: false, // test true too?
            version: 2,
            version_rolling_allowed: false, // test true too?
            merkle_path: Seq0255::new(Vec::<U256>::new()).unwrap(),
            coinbase_tx_prefix,
            coinbase_tx_suffix,
        };
        let target: Target = ([
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0b_0001_0000,
            0_u8,
        ])
        .try_into()
        .unwrap();

        let channel = StandardChannel {
            channel_id,
            group_id: 0,
            target,
            extranonce: mining_sv2::Extranonce::new(),
        };
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);
        dispatcher.on_new_extended_mining_job(&extended, &channel);
        let x = 1;
        assert_eq!(1, x);
    }

    #[ignore]
    #[test]
    fn updates_group_channel_job_dispatcher_on_new_prev_hash() -> Result<(), Error> {
        let message = SetNewPrevHash {
            channel_id: 0,
            job_id: 0,
            prev_hash: u256_from_int(45_u32),
            min_ntime: 0,
            nbits: 0,
        };
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);

        // TODO: fails on self.future_jobs unwrap in the first line of the on_new_prev_hash fn
        let actual = dispatcher.on_new_prev_hash(&message)?;
        // let actual_prev_hash: U256<'static> = u256_from_int(tt);
        let expect_prev_hash: Vec<u8> = dispatcher.prev_hash.to_vec();
        // assert_eq!(expect_prev_hash, dispatcher.prev_hash);
        //
        assert_eq!(expect_prev_hash, dispatcher.prev_hash);

        let x = 1;
        assert_eq!(1, x);

        Ok(())
    }

    #[test]
    fn fails_to_update_group_channel_job_dispatcher_on_new_prev_hash_if_no_future_jobs() {
        let message = SetNewPrevHash {
            channel_id: 0,
            job_id: 0,
            prev_hash: u256_from_int(45_u32),
            min_ntime: 0,
            nbits: 0,
        };
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);

        let err = dispatcher.on_new_prev_hash(&message).unwrap_err();
        assert_eq!(
            err.to_string(),
            "GroupChannelJobDispatcher does not have any future jobs"
        );
        // match actual {
        //     Ok(a) => assert!(true),
        //     Err(e) => assert!(false),
        // };
    }
}
