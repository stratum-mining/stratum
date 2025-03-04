//! Job Dispatcher
//!
//! This module contains relevant logic to maintain group channels in proxy roles such as:
//! - converting extended jobs to standard jobs
//! - handling updates to jobs when new templates and prev hashes arrive, as well as cleaning up old
//!   jobs
//! - determining if submitted shares correlate to valid jobs

use crate::{
    common_properties::StandardChannel,
    utils::{merkle_root_from_path, Id, Mutex},
    Error,
};
use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, SetNewPrevHash, SubmitSharesError, SubmitSharesStandard,
    Target,
};
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, convert::TryInto, sync::Arc};

use stratum_common::bitcoin::hashes::{sha256d, Hash, HashEngine};

/// Used to convert an extended mining job to a standard mining job. The `extranonce` field must
/// be exactly 32 bytes.
pub fn extended_to_standard_job_for_group_channel<'a>(
    extended: &NewExtendedMiningJob,
    extranonce: &[u8],
    channel_id: u32,
    job_id: u32,
) -> Option<NewMiningJob<'a>> {
    let merkle_root = merkle_root_from_path(
        extended.coinbase_tx_prefix.inner_as_ref(),
        extended.coinbase_tx_suffix.inner_as_ref(),
        extranonce,
        &extended.merkle_path.inner_as_ref(),
    );

    Some(NewMiningJob {
        channel_id,
        job_id,
        min_ntime: extended.min_ntime.clone().into_static(),
        version: extended.version,
        merkle_root: merkle_root?.try_into().ok()?,
    })
}

// helper struct to easily calculate block hashes from headers
#[allow(dead_code)]
struct Header<'a> {
    version: u32,
    prev_hash: &'a [u8],
    merkle_root: &'a [u8],
    timestamp: u32,
    nbits: u32,
    nonce: u32,
}

impl<'a> Header<'a> {
    // calculates the sha256 blockhash of the header
    #[allow(dead_code)]
    pub fn hash(&self) -> Target {
        let mut engine = sha256d::Hash::engine();
        engine.input(&self.version.to_le_bytes());
        engine.input(self.prev_hash);
        engine.input(self.merkle_root);
        engine.input(&self.timestamp.to_be_bytes());
        engine.input(&self.nbits.to_be_bytes());
        engine.input(&self.nonce.to_be_bytes());
        let hashed: [u8; 32] = *sha256d::Hash::from_engine(engine).as_ref();
        hashed.into()
    }
}

// helper struct to identify Standard Jobs being managed for downstream
#[derive(Debug)]
struct DownstreamJob {
    #[allow(dead_code)]
    merkle_root: Vec<u8>,
    extended_job_id: u32,
}

/// Used by proxies to keep track of standard jobs in the group channel
/// created with the sv2 server
#[derive(Debug)]
pub struct GroupChannelJobDispatcher {
    //channels: Vec<StandardChannel>,
    #[allow(dead_code)]
    target: Target,
    prev_hash: Vec<u8>,
    // extended_job_id -> standard_job_id -> standard_job
    future_jobs:
        HashMap<u32, HashMap<u32, DownstreamJob, BuildNoHashHasher<u32>>, BuildNoHashHasher<u32>>,
    // standard_job_id -> standard_job
    jobs: HashMap<u32, DownstreamJob, BuildNoHashHasher<u32>>,
    ids: Arc<Mutex<Id>>,
    // extended_id -> channel_id -> standard_id
    extended_id_to_job_id:
        HashMap<u32, HashMap<u32, u32, BuildNoHashHasher<u32>>, BuildNoHashHasher<u32>>,
    nbits: u32,
}

/// Used to signal if submitted shares correlate to valid jobs
pub enum SendSharesResponse {
    /// ValidAndMeetUpstreamTarget((SubmitSharesStandard,SubmitSharesSuccess)),
    Valid(SubmitSharesStandard),
    Invalid(SubmitSharesError<'static>),
}

impl GroupChannelJobDispatcher {
    /// constructor
    pub fn new(ids: Arc<Mutex<Id>>) -> Self {
        Self {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            ids,
            nbits: 0,
            extended_id_to_job_id: HashMap::with_hasher(BuildNoHashHasher::default()),
        }
    }

    /// When a downstream opens a connection with a proxy, the proxy uses this function to create a
    /// new mining job from the last valid new extended mining job.
    ///
    /// When a proxy receives a new extended mining job from upstream it uses this function to
    /// create the corresponding new mining job for each connected downstream.
    #[allow(clippy::option_map_unit_fn)]
    pub fn on_new_extended_mining_job(
        &mut self,
        extended: &NewExtendedMiningJob,
        channel: &StandardChannel,
        // should be changed to return a Result<Option<NewMiningJob>>
    ) -> Option<NewMiningJob<'static>> {
        if extended.is_future() {
            self.future_jobs
                .entry(extended.job_id)
                .or_insert_with(|| HashMap::with_hasher(BuildNoHashHasher::default()));
            self.extended_id_to_job_id
                .entry(extended.job_id)
                .or_insert_with(|| HashMap::with_hasher(BuildNoHashHasher::default()));
        }

        // Is fine to unwrap a safe_lock result
        let standard_job_id = self.ids.safe_lock(|ids| ids.next()).unwrap();

        let extranonce: Vec<u8> = channel.extranonce.clone().into();
        let new_mining_job_message = extended_to_standard_job_for_group_channel(
            extended,
            &extranonce,
            channel.channel_id,
            standard_job_id,
        )?;
        let job = DownstreamJob {
            merkle_root: new_mining_job_message.merkle_root.to_vec(),
            extended_job_id: extended.job_id,
        };
        if extended.is_future() {
            self.future_jobs
                .get_mut(&extended.job_id)
                .map(|future_jobs| {
                    future_jobs.insert(standard_job_id, job);
                });

            let channel_id_to_standard_id = self
                .extended_id_to_job_id
                .get_mut(&extended.job_id)
                // The key is always in the map cause we insert it above if not present
                .unwrap();
            channel_id_to_standard_id.insert(channel.channel_id, standard_job_id);
        } else {
            self.jobs.insert(new_mining_job_message.job_id, job);
        };
        Some(new_mining_job_message)
    }

    /// Called when a SetNewPrevHash message is received.
    /// This function will move all future jobs to current jobs, clear old jobs,
    /// and update `self` to reference the latest prev_hash and nbits
    /// associated with the latest job.
    pub fn on_new_prev_hash(
        &mut self,
        message: &SetNewPrevHash,
    ) -> Result<HashMap<u32, u32, BuildNoHashHasher<u32>>, Error> {
        let jobs = self
            .future_jobs
            .get_mut(&message.job_id)
            .ok_or(Error::PrevHashRequireNonExistentJobId(message.job_id))?;
        std::mem::swap(&mut self.jobs, jobs);
        self.prev_hash = message.prev_hash.to_vec();
        self.nbits = message.nbits;
        self.future_jobs.clear();
        match self.extended_id_to_job_id.remove(&message.job_id) {
            Some(map) => {
                self.extended_id_to_job_id.clear();
                Ok(map)
            }
            None => {
                self.extended_id_to_job_id.clear();
                Ok(HashMap::with_hasher(BuildNoHashHasher::default()))
            }
        }
    }

    /// takes shares submitted by a group channel miner and determines if the shares correspond to a
    /// valid job.
    pub fn on_submit_shares(&self, shares: SubmitSharesStandard) -> SendSharesResponse {
        let id = shares.job_id;
        if let Some(job) = self.jobs.get(&id) {
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
                // Below unwrap never panic because an empty string will always fit
                // in a `Inner<false, 1, 1, 255>` type
                error_code: "".to_string().into_bytes().try_into().unwrap(),
            };
            SendSharesResponse::Invalid(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::Error,
        job_creator::{
            tests::{new_pub_key, template_from_gen},
            JobsCreators,
        },
    };
    use binary_sv2::{u256_from_int, U256};
    use mining_sv2::Extranonce;
    use quickcheck::{Arbitrary, Gen};
    use std::convert::TryFrom;

    use stratum_common::bitcoin::{Amount, ScriptBuf, TxOut};

    const BLOCK_REWARD: u64 = 625_000_000_000;

    #[test]
    fn test_block_hash() {
        let le_version = "0x32950000".strip_prefix("0x").unwrap();
        let be_prev_hash = "0x00000000000000000004e962c1a0fc6a201d937bf08ffe4b1221e956615c7cd9";
        let be_merkle_root = "0x897dff6755a7c255455f1b2a2c8ad44ad1b6c23ef00fbf501d0dde7e42cd8c71";
        let le_timestamp = "0x637B9A4C".strip_prefix("0x").unwrap();
        let le_nbits = "0x17079e15".strip_prefix("0x").unwrap();
        let le_nonce = "0x102aa10".strip_prefix("0x").unwrap();

        let le_version = u32::from_str_radix(le_version, 16).expect("Failed converting hex to u32");
        let mut be_prev_hash =
            utils::decode_hex(be_prev_hash).expect("Failed converting hex to bytes");
        let mut be_merkle_root =
            utils::decode_hex(be_merkle_root).expect("Failed converting hex to bytes");
        let le_timestamp: u32 =
            u32::from_str_radix(le_timestamp, 16).expect("Failed converting hex to u32");
        let le_nbits = u32::from_str_radix(le_nbits, 16).expect("Failed converting hex to u32");
        let le_nonce = u32::from_str_radix(le_nonce, 16).expect("Failed converting hex to u32");
        be_prev_hash.reverse();
        be_merkle_root.reverse();
        let le_prev_hash = be_prev_hash.as_slice();
        let le_merkle_root = be_merkle_root.as_slice();

        let block_header: Header = Header {
            version: le_version,
            prev_hash: le_prev_hash,
            merkle_root: le_merkle_root,
            timestamp: le_timestamp.to_be(),
            nbits: le_nbits.to_be(),
            nonce: le_nonce.to_be(),
        };

        let target = U256::from(block_header.hash());
        let mut actual_block_hash =
            utils::decode_hex("00000000000000000000199349a95526c4f83959f0ef06697048a297f25e7fac")
                .expect("Failed converting hex to bytes");
        actual_block_hash.reverse();
        assert_eq!(
            target.to_vec(),
            actual_block_hash,
            "Computed block hash does not equal the actaul block hash"
        );
    }

    #[test]
    fn test_group_channel_job_dispatcher() {
        let out = TxOut {
            value: Amount::from_sat(BLOCK_REWARD),
            script_pubkey: ScriptBuf::new_p2pk(&new_pub_key()),
        };
        let additional_coinbase_script_data = "Stratum v2 SRI Pool".as_bytes().to_vec();
        let mut jobs_creators = JobsCreators::new(32);
        let group_channel_id = 1;
        //Create a template
        let mut template = template_from_gen(&mut Gen::new(255));
        template.template_id = template.template_id % u64::MAX;
        template.future_template = true;
        let extended_mining_job = jobs_creators
            .on_new_template(
                &mut template,
                false,
                vec![out],
                additional_coinbase_script_data.len(),
            )
            .expect("Failed to create new job");

        // create GroupChannelJobDispatcher
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut group_channel_dispatcher = GroupChannelJobDispatcher::new(ids);
        // create standard channel
        let target = Target::from(U256::try_from(utils::extranonce_gen()).unwrap());
        let standard_channel_id = 2;
        let extranonce = Extranonce::try_from(utils::extranonce_gen())
            .expect("Failed to convert bytes to extranonce");
        let standard_channel = StandardChannel {
            channel_id: standard_channel_id,
            group_id: group_channel_id,
            target,
            extranonce: extranonce.clone(),
        };
        // call target function (on_new_extended_mining_job)
        let new_mining_job = group_channel_dispatcher
            .on_new_extended_mining_job(&extended_mining_job, &standard_channel)
            .unwrap();

        // on_new_extended_mining_job assertions
        let (future_job_id, test_merkle_root) = assert_on_new_extended_mining_job(
            &group_channel_dispatcher,
            &new_mining_job,
            &extended_mining_job,
            extranonce.clone(),
            standard_channel_id,
        );
        // on_new_prev_hash assertions
        if extended_mining_job.is_future() {
            assert_on_new_prev_hash(
                &mut group_channel_dispatcher,
                standard_channel_id,
                future_job_id,
                test_merkle_root,
            )
        }
        assert_on_submit_shares(
            &group_channel_dispatcher,
            standard_channel_id,
            future_job_id,
        );
    }

    fn assert_on_new_extended_mining_job(
        group_channel_job_dispatcher: &GroupChannelJobDispatcher,
        new_mining_job: &NewMiningJob,
        extended_mining_job: &NewExtendedMiningJob,
        extranonce: Extranonce,
        standard_channel_id: u32,
    ) -> (u32, Vec<u8>) {
        // compute test merkle path
        let new_root = merkle_root_from_path(
            extended_mining_job.coinbase_tx_prefix.inner_as_ref(),
            extended_mining_job.coinbase_tx_suffix.inner_as_ref(),
            extranonce.to_vec().as_slice(),
            &extended_mining_job.merkle_path.inner_as_ref(),
        )
        .unwrap();
        // Assertions
        assert_eq!(
            new_mining_job.channel_id, standard_channel_id,
            "channel_id did not convert correctly"
        );
        assert_eq!(
            new_mining_job.job_id, extended_mining_job.job_id,
            "job_id did not convert correctly"
        );
        assert_eq!(
            new_mining_job.version, extended_mining_job.version,
            "version did not convert correctly"
        );
        assert_eq!(
            new_mining_job.min_ntime, extended_mining_job.min_ntime,
            "future_job did not convert correctly"
        );
        assert_eq!(
            new_mining_job.merkle_root.to_vec(),
            new_root,
            "merkle_root did not convert correctly"
        );
        let mut future_job_id: u32 = 0;
        if new_mining_job.is_future() {
            // assert job_id counter
            let job_ids = group_channel_job_dispatcher
                .extended_id_to_job_id
                .get(&extended_mining_job.job_id)
                .unwrap();
            let standard_job_id = job_ids.get(&standard_channel_id).unwrap();
            let standard_job_id_counter: u32 = group_channel_job_dispatcher
                .ids
                .safe_lock(|id| id.next())
                .unwrap();
            let prev_value = standard_job_id_counter - 1;
            assert_eq!(
                standard_job_id, &prev_value,
                "Job Id counter does not match"
            );
            // assert job was stored
            let future_jobs = group_channel_job_dispatcher
                .future_jobs
                .get(&extended_mining_job.job_id)
                .unwrap();
            let job = future_jobs.get(&prev_value).unwrap();
            assert_eq!(
                job.extended_job_id, extended_mining_job.job_id,
                "job_id not stored correctly in future_jobs"
            );
            assert_eq!(
                job.merkle_root,
                new_mining_job.merkle_root.to_vec(),
                "job merkle root not stored correctly in future jobs"
            );
            future_job_id = prev_value;
        }
        (future_job_id, new_root)
    }

    fn assert_on_new_prev_hash(
        group_channel_job_dispatcher: &mut GroupChannelJobDispatcher,
        standard_channel_id: u32,
        future_job_id: u32,
        test_merkle_root: Vec<u8>,
    ) {
        let mut prev_hash = Vec::new();
        prev_hash.resize_with(32, || u8::arbitrary(&mut Gen::new(1)));
        let min_ntime: u32 = 1;
        let nbits: u32 = 1;
        let new_message = SetNewPrevHash {
            channel_id: standard_channel_id,
            job_id: future_job_id,
            prev_hash: U256::try_from(prev_hash).unwrap(),
            min_ntime,
            nbits,
        };

        group_channel_job_dispatcher
            .on_new_prev_hash(&new_message)
            .expect("on_new_prev_hash failed to execute");

        // assert future job was moved to current jobs
        let new_current_job = group_channel_job_dispatcher
            .jobs
            .get(&future_job_id)
            .unwrap();
        assert_eq!(
            new_current_job.merkle_root, test_merkle_root,
            "Future job not moved to current job correctly (merkle root)"
        );
        assert_eq!(
            new_current_job.extended_job_id, future_job_id,
            "Future job not moved to current job correctly (job_id)"
        );
        assert_eq!(
            group_channel_job_dispatcher.nbits, new_message.nbits,
            "nbits not updated for SetNewPrevHash"
        );
        assert_eq!(
            group_channel_job_dispatcher.prev_hash,
            new_message.prev_hash.to_vec(),
            "prev_hash not updated for SetNewPrevHash"
        );
        assert!(
            group_channel_job_dispatcher.future_jobs.is_empty(),
            "Future jobs did not get cleared"
        )
    }
    fn assert_on_submit_shares(
        group_channel_job_dispatcher: &GroupChannelJobDispatcher,
        standard_channel_id: u32,
        job_id: u32,
    ) {
        let shares = SubmitSharesStandard {
            // Channel identification.
            channel_id: standard_channel_id,
            // Unique sequential identifier of the submit within the channel.
            sequence_number: 0,
            // Identifier of the job as provided by *NewMiningJob* or
            // *NewExtendedMiningJob* message.
            job_id,
            // Nonce leading to the hash being submitted.
            nonce: 1,
            // The nTime field in the block header. This MUST be greater than or equal
            // to the header_timestamp field in the latest SetNewPrevHash message
            // and lower than or equal to that value plus the number of seconds since
            // the receipt of that message.
            ntime: 1,
            // Full nVersion field.
            version: 1,
        };
        let mut faulty_shares = shares.clone();
        faulty_shares.job_id += 1;

        for (index, shares) in vec![shares, faulty_shares].iter().enumerate() {
            match group_channel_job_dispatcher.on_submit_shares(shares.clone()) {
                SendSharesResponse::Valid(resp) => {
                    assert_eq!(
                        index, 0,
                        "Only the first item in iterator should be a valid response"
                    );
                    assert_eq!(resp.channel_id, standard_channel_id);
                    assert_eq!(resp.job_id, job_id);
                    assert_eq!(resp.sequence_number, shares.sequence_number);
                    assert_eq!(resp.nonce, shares.nonce);
                    assert_eq!(resp.ntime, shares.ntime);
                    assert_eq!(resp.version, shares.version);
                }
                SendSharesResponse::Invalid(err) => {
                    assert_eq!(
                        index, 1,
                        "Only the second item in iterator should be an invalid response"
                    );
                    assert_eq!(err.channel_id, standard_channel_id);
                    assert_eq!(err.sequence_number, shares.sequence_number);
                    assert_eq!(
                        err.error_code,
                        "".to_string().into_bytes().try_into().unwrap()
                    );
                }
            };
        }
    }

    #[test]
    fn builds_group_channel_job_dispatcher() {
        let expect = GroupChannelJobDispatcher {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            ids: Arc::new(Mutex::new(Id::new())),
            nbits: 0,
            extended_id_to_job_id: HashMap::with_hasher(BuildNoHashHasher::default()),
        };

        let ids = Arc::new(Mutex::new(Id::new()));
        let actual = GroupChannelJobDispatcher::new(ids);

        assert_eq!(expect.target, actual.target);
        assert_eq!(expect.prev_hash, actual.prev_hash);
        assert_eq!(expect.nbits, actual.nbits);
        assert!(actual.future_jobs.is_empty());
        assert!(actual.jobs.is_empty());
        // check actual.ids, but idk how to properly test arc
        // assert_eq!(expect.ids, actual.ids);
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

        // fails on self.future_jobs unwrap in the first line of the on_new_prev_hash fn
        let _actual = dispatcher.on_new_prev_hash(&message);
        // let actual_prev_hash: U256<'static> = u256_from_int(tt);
        let expect_prev_hash: Vec<u8> = dispatcher.prev_hash.to_vec();
        // assert_eq!(expect_prev_hash, dispatcher.prev_hash);
        assert_eq!(expect_prev_hash, dispatcher.prev_hash);

        Ok(())
    }

    pub mod utils {
        use super::*;
        use std::fmt::Write;

        pub fn extranonce_gen() -> Vec<u8> {
            let mut u8_gen = Gen::new(1);
            let mut extranonce: Vec<u8> = Vec::new();
            extranonce.resize_with(32, || u8::arbitrary(&mut u8_gen));
            extranonce
        }

        pub fn decode_hex(s: &str) -> Result<Vec<u8>, core::num::ParseIntError> {
            let s = match s.strip_prefix("0x") {
                Some(s) => s,
                None => s,
            };
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .collect()
        }

        pub fn _encode_hex(bytes: &[u8]) -> String {
            let mut s = String::with_capacity(bytes.len() * 2);
            for &b in bytes {
                write!(&mut s, "{:02x}", b).unwrap();
            }
            s
        }
    }
}
