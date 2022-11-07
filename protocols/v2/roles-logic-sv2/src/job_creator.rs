use crate::{utils::Id, Error};
use binary_sv2::B064K;
use bitcoin::{
    blockdata::{
        script::Script,
        transaction::{OutPoint, Transaction, TxIn, TxOut},
    },
    util::psbt::serialize::{Deserialize, Serialize},
};
pub use bitcoin::{
    secp256k1::SecretKey,
    util::ecdsa::{PrivateKey, PublicKey},
};
use mining_sv2::NewExtendedMiningJob;
use std::{collections::HashMap, convert::TryInto};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

const EXTRANONCE_LEN: usize = 32;

/// CB_PREFIX_LEN is the default and consistent byte length of the first part
/// of a serialized coinbase transaction. It includes all invariants and excludes
/// all variants.
///
/// Invariants (in order):
///   - version: 4 bytes
///   - number of inputs: 1 byte
///   - previous output hash + previous output index: 36 bytes
///   - scriptSig length: 1 byte
///   - block height length: 1 byte
const CB_PREFIX_LEN: usize = 43;

/// SCRIPT_SIG_LEN_INDEX is the index position of the scriptSig length in a serialized
/// non-segwit coinbase transaction. For a Segwit transaction, this index position
/// will need an additional 2 bytes (SEGWIT_FLAG_LEN).
///
/// The scriptSig length, includes all of the following fields:
///     - block height length (1 byte)
///     - block height (variable according to block height length value)
///     - extra nonce (variable according to the remaining bytes after the 2 fields above)
const SCRIPT_SIG_LEN_INDEX: usize = 41;

/// SEGWIT_FLAG_LEN is the byte length of the optional Segwit flag in a coinbase
/// transaction.
const SEGWIT_FLAG_LEN: usize = 2;

/// Hardcoded value if/when a spec change is approved to send this value from the
/// TemplateProvider: https://github.com/stratum-mining/sv2-spec/pull/15
///
/// The WITNESS_RESERVE_VALUE is used to create/validate a witness commitment given:
/// SHA256^2(witness_reserve_value, witness_root);
const WITNESS_RESERVE_VALUE: [u8; 32] = [0x00; 32];

/// Used by pool one for each group channel
/// extended and standard channel not supported
#[derive(Debug)]
struct JobCreator {
    group_channel_id: u32,
    job_ids: Id,
    version_rolling_allowed: bool,
    template_id_to_job_id: HashMap<u64, u32>,
}

impl JobCreator {
    fn new_extended_job(
        &mut self,
        new_template: &mut NewTemplate,
        coinbase_outputs: &[TxOut],
    ) -> Result<NewExtendedMiningJob<'static>, Error> {
        let script_prefix = new_template.coinbase_prefix.to_vec();
        // Is ok to panic here cause condition will be always true when not in a test chain
        // (regtest ecc ecc)
        assert!(
            script_prefix.len() > 3,
            "Bitcoin blockchain should be at least 16 block long"
        );
        let bip34_len = script_prefix[1] as usize;
        let bip34_bytes = script_prefix[1..2 + bip34_len].to_vec();

        // NOTE: There maybe be better ways to determine if the NewTemplate
        // has segwit inputs. In this case, we are assuming extra coinbase_tx_outputs
        // means a witness commitment.
        let segwit_flag = new_template.coinbase_tx_outputs_count > 0;

        let coinbase = self.coinbase(
            bip34_bytes,
            new_template
                .coinbase_tx_version
                .try_into()
                .expect("invalid version"),
            new_template.coinbase_tx_locktime,
            new_template.coinbase_tx_input_sequence,
            coinbase_outputs,
            segwit_flag,
        );
        let new_extended_mining_job: NewExtendedMiningJob<'static> = NewExtendedMiningJob {
            channel_id: self.group_channel_id,
            job_id: self.job_ids.next(),
            future_job: new_template.future_template,
            version: new_template.version,
            version_rolling_allowed: self.version_rolling_allowed,
            merkle_path: new_template.merkle_path.clone().into_static(),
            coinbase_tx_prefix: Self::coinbase_tx_prefix(&coinbase, bip34_len, segwit_flag)?,
            coinbase_tx_suffix: Self::coinbase_tx_suffix(&coinbase, segwit_flag)?,
        };
        self.template_id_to_job_id
            .insert(new_template.template_id, new_extended_mining_job.job_id);
        Ok(new_extended_mining_job)
    }

    fn get_job_id(&self, template_id: u64) -> Option<u32> {
        self.template_id_to_job_id.get(&template_id).copied()
    }

    fn coinbase_tx_prefix(
        coinbase: &Transaction,
        cb_tx_block_height_len: usize,
        segwit: bool,
    ) -> Result<B064K<'static>, Error> {
        let encoded = coinbase.serialize();

        let segwit_byte_flag = if segwit { SEGWIT_FLAG_LEN } else { 0 };
        let prefix = encoded[0..CB_PREFIX_LEN + segwit_byte_flag + cb_tx_block_height_len].to_vec();

        prefix.try_into().map_err(Error::BinarySv2Error)
    }

    fn coinbase_tx_suffix(coinbase: &Transaction, segwit: bool) -> Result<B064K<'static>, Error> {
        let encoded = coinbase.serialize();

        let segwit_byte_flag = if segwit { SEGWIT_FLAG_LEN } else { 0 };
        let script_sig_size = encoded[SCRIPT_SIG_LEN_INDEX + segwit_byte_flag];

        let suffix =
            encoded[CB_PREFIX_LEN + segwit_byte_flag + (script_sig_size - 1) as usize..].to_vec();

        suffix.try_into().map_err(Error::BinarySv2Error)
    }

    /// coinbase_tx_input_script_prefix: extranonce prefix (script length + bip34 block height) provided by the node
    fn coinbase(
        &self,
        mut bip34_bytes: Vec<u8>,
        version: i32,
        lock_time: u32,
        sequence: u32,
        coinbase_outputs: &[TxOut],
        segwit: bool,
    ) -> Transaction {
        bip34_bytes.extend_from_slice(&[0; EXTRANONCE_LEN]);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: bip34_bytes.into(),
            sequence,
            witness: if segwit {
                vec![WITNESS_RESERVE_VALUE.to_vec()]
            } else {
                vec![]
            },
        };

        Transaction {
            version,
            lock_time,
            input: vec![tx_in],
            output: coinbase_outputs.to_vec(),
        }
    }
}

/// Used by pool add a JobCreator for each group channel
/// extended and standard channel not supported
#[derive(Debug)]
pub struct JobsCreators {
    jobs_creators: Vec<JobCreator>,
    /// Computed by the pool
    coinbase_outputs: Vec<TxOut>,
    block_reward_staoshi: u64,
    pub_key: PublicKey,
    lasts_new_template: Vec<NewTemplate<'static>>,
    //last_prev_hash: Pr
}

impl JobsCreators {
    pub fn new(block_reward_staoshi: u64, pub_key: PublicKey) -> Option<Self> {
        Some(Self {
            jobs_creators: vec![],
            coinbase_outputs: vec![Self::new_output(block_reward_staoshi, pub_key)?],
            block_reward_staoshi,
            pub_key,
            lasts_new_template: Vec::new(),
        })
    }

    fn new_output(block_reward_staoshi: u64, pub_key: PublicKey) -> Option<TxOut> {
        let script_pubkey = Script::new_v0_wpkh(&pub_key.wpubkey_hash()?);
        Some(TxOut {
            value: block_reward_staoshi,
            script_pubkey,
        })
    }

    pub fn new_outputs(&self, block_reward_staoshi: u64) -> Vec<TxOut> {
        // safe unwrap cause pub key in self is compressed
        vec![Self::new_output(block_reward_staoshi, self.pub_key).unwrap()]
    }

    pub fn on_new_template(
        &mut self,
        template: &mut NewTemplate,
    ) -> Result<HashMap<u32, NewExtendedMiningJob<'static>>, Error> {
        if template.coinbase_tx_value_remaining != self.block_reward_staoshi {
            self.block_reward_staoshi = template.coinbase_tx_value_remaining;
            self.coinbase_outputs = self.new_outputs(template.coinbase_tx_value_remaining);
        }

        if template.coinbase_tx_outputs_count > 0 {
            self.coinbase_outputs = self.new_outputs(template.coinbase_tx_value_remaining);

            for output in template.coinbase_tx_outputs.inner_as_ref() {
                self.coinbase_outputs
                    .push(TxOut::deserialize(output.inner_as_ref()).unwrap());
            }
        }

        let mut new_extended_jobs = HashMap::new();
        for creator in &mut self.jobs_creators {
            let job = creator.new_extended_job(template, &self.coinbase_outputs)?;
            new_extended_jobs.insert(job.channel_id, job);
        }
        self.lasts_new_template.push(template.as_static());

        Ok(new_extended_jobs)
    }

    fn reset_new_templates(&mut self, template: Option<NewTemplate<'static>>) {
        match template {
            Some(t) => self.lasts_new_template = vec![t],
            None => self.lasts_new_template = vec![],
        }
    }

    pub fn on_new_prev_hash(&mut self, prev_hash: &SetNewPrevHash<'static>) {
        let template: Vec<NewTemplate<'static>> = self
            .lasts_new_template
            .clone()
            .into_iter()
            .filter(|a| a.template_id == prev_hash.template_id)
            .collect();
        match template.len() {
            0 => self.reset_new_templates(None),
            1 => self.reset_new_templates(Some(template[0].clone())),
            // TODO how many templates can we have at max
            _ => todo!("{:#?}", template.len()),
        }
    }

    pub fn new_group_channel(
        &mut self,
        group_channel_id: u32,
        version_rolling_allowed: bool,
    ) -> Result<Vec<(NewExtendedMiningJob<'static>, u64)>, Error> {
        let mut jc = JobCreator {
            group_channel_id,
            job_ids: Id::new(),
            version_rolling_allowed,
            template_id_to_job_id: HashMap::new(),
        };
        let mut res = Vec::new();
        for mut template in self.lasts_new_template.clone() {
            res.push((
                jc.new_extended_job(&mut template, &self.coinbase_outputs)?,
                template.template_id,
            ));
        }
        self.jobs_creators.push(jc);
        Ok(res)
    }

    pub fn job_id_from_template(&self, template_id: u64, group_id: u32) -> Option<u32> {
        for jc in &self.jobs_creators {
            if jc.group_channel_id == group_id {
                return jc.get_job_id(template_id);
            }
        }
        None
    }
}

mod test {
    use super::*;
    pub use bitcoin::consensus::encode::deserialize;

    /// Convenience struct for asserting that the JobCreator is able to parse
    /// the serialized bytes of a coinbase transaction correctly.
    struct CoinbaseTestHarness<'a> {
        cb_bytes: &'a [u8],
        segwit: bool,
        block_height_len: usize,

        /// The last 3 bytes expected of a coinbase_prefix parsed by the JobCreator.
        /// This is to assert and catch off by one errors at the end of the prefix.
        expected_prefix_end: &'a [u8],

        /// The first 3 bytes expected of a coinbase_suffix parsed by the JobCreator.
        /// This is to assert and catch off bye one errors and also be able to handle
        /// different extranonce lengths.
        expected_suffix_start: &'a [u8],
    }

    impl<'a> CoinbaseTestHarness<'a> {
        /// Test the prefix and suffix are sliced correctly.
        fn test_coinbase_prefix_suffix(&self) {
            let cb_tx: Transaction = deserialize(&self.cb_bytes).unwrap();

            let segwit_flag = if self.segwit { SEGWIT_FLAG_LEN } else { 0 };

            // The coinbase prefix should be sliced to the end of the block height field.
            let cb_prefix =
                JobCreator::coinbase_tx_prefix(&cb_tx, self.block_height_len, self.segwit).unwrap();
            assert_eq!(
                &self.cb_bytes[0..CB_PREFIX_LEN + segwit_flag + self.block_height_len],
                &cb_prefix.inner_as_ref()[..]
            );

            // The coinbase suffix should be sliced from the sequence number (after
            // the extranonce) until the end.
            let script_sig_len: usize =
                (self.cb_bytes[SCRIPT_SIG_LEN_INDEX + segwit_flag] - 1).into();

            let cb_suffix = JobCreator::coinbase_tx_suffix(&cb_tx, self.segwit).unwrap();
            assert_eq!(
                &self.cb_bytes[CB_PREFIX_LEN + segwit_flag + script_sig_len..],
                &cb_suffix.inner_as_ref()[..]
            );

            // Explicity test for the expected end of prefix bytes and beginning
            // of suffix bytes to catch previous off by one errors.
            let prefix_len = cb_prefix.inner_as_ref().len();
            assert_eq!(
                &cb_prefix.inner_as_ref()[prefix_len - 3..],
                self.expected_prefix_end
            );
            assert_eq!(&cb_suffix.inner_as_ref()[0..3], self.expected_suffix_start);
        }
    }

    #[test]
    fn parse_coinbase_tx() {
        // Segwit Regtest Block - Height: 256
        #[rustfmt::skip]
        let regtest_segwit_cb_bytes = [
            2, 0, 0, 0, // version
            0, 1, // segwit optional flag
            1, // number of inputs
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // prev out
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            255, 255, 255, 255, // prev out index
            35, // scriptSig len
            2, // block height len
            0, 1, // block height
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // extranonce
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            255, 255, 255, 255, // sequence number
            2, // number of outputs
            0, 249, 2, 149, 0, 0, 0, 0, // output value
            22, // scriptPubKey len
            0, 20, 83, 18, 96, 170, 42, 25, 158, 34, 140, 83, 125, 250, 66, 200, // scriptPubKey
            43, 234, 44, 124, 31, 77,
            0, 0, 0, 0, // locktime
            0, 0, 0, 0, // start of coinbase witness commitment output
            38, // length of following bytes
            106, // OP_RETURN
            36, // Push 36 bytes
            170, 33, 169, 237, // commitment header
            226, 246, 28, 63, 113, 209, 222, 253, 63, 169, 153, 223, 163, 105, // commitment hash and extra data
            83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139, 235, 216, 54, 151,
            78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ];
        CoinbaseTestHarness {
            cb_bytes: &regtest_segwit_cb_bytes,
            segwit: true,
            block_height_len: 2,
            expected_prefix_end: &[2, 0, 1],
            expected_suffix_start: &[255, 255, 255],
        }
        .test_coinbase_prefix_suffix();

        // Height: 761362
        // https://blockstream.info/tx/eede27543f086abd612b87096a7216229d4c736d39bbdfd4fefc1455f427997f
        #[rustfmt::skip]
        let segwit_cb_bytes = [
            2, 0, 0, 0, // version
            0, 1, // segwit optional flag
            1, // number of inputs
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // prev out
            0, 0, 0, 0, 0, 0, 0, 0, 0,
            255, 255, 255, 255, // prev out index
            49, // scriptSig len
            3, // block height len
            18, 158, 11, // block height
            4, 122, 0, 98, 99, 47, 70, 111, 117, 110, 100, 114, 121, 32, 85, 83, // extranonce
            65, 32, 80, 111, 111, 108, 32, 35, 100, 114, 111, 112, 103, 111, 108,
            100, 47, 13, 85, 98, 0, 128, 4, 0, 0, 0, 0, 0, 0,
            255, 255, 255, 255, // sequence number
            2, // number of outputs
            6, 149, 39, 38, 0, 0, 0, 0, // output value
            25, // scriptPubKey len
            118, 169, 20, 14, 110, 210, 249, 94, 138, 127, 49, 49, 83, 84, 114, // scriptPubKey
            225, 0, 154, 28, 119, 189, 1, 229, 136, 172,
            0, 0, 0, 0, // lock time
            0, 0, 0, 0, // start of coinbase witness commitment output
            38,  // length of following bytes
            106, // OP_RETURN
            36,  // Push 36 bytes
            170, 33, 169, 237, // commitment header
            209, 182, 201, 196, 24, 245, 120, 15, 237, 192, 164, 203, 138, 101, // commitment hash and extra data
            201, 95, 159, 55, 243, 248, 85, 219, 230, 159, 196, 135, 213, 73,
            79, 164, 227, 50, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        CoinbaseTestHarness {
            cb_bytes: &segwit_cb_bytes,
            segwit: true,
            block_height_len: 3,
            expected_prefix_end: &[18, 158, 11],
            expected_suffix_start: &[255, 255, 255],
        }
        .test_coinbase_prefix_suffix();

        // Height: 626507
        // https://blockstream.info/tx/629509d4018a810f7af1e77d5abc5051c09e5b0df9552ca8ab329e0fa5b317cd
        #[rustfmt::skip]
        let non_segwit_cb_bytes = [
            1, 0, 0, 0, // version
            1, // tx input count
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, // prev out
            255, 255, 255, 255, // prev out index
            83,  // scriptSig len
            3,   // block height len
            113, 104, 9, // block height
            4, 159, 32, 63, 94, 250, 190, 109, 109, 121, 10, 234, 181, 125, 12, // extranonce
            44, 109, 178, 42, 205, 243, 18, 32, 72, 100, 170, 110, 8, 4, 210,
            54, 188, 251, 243, 25, 63, 33, 106, 192, 174, 248, 4, 0, 0, 0, 0,
            0, 0, 0, 8, 24, 0, 82, 194, 215, 5, 0, 0, 20, 47, 112, 114, 111,
            104, 97, 115, 104, 105, 110, 103, 46, 99, 111, 109, 155, 29, 2, 0, 47,
            0, 0, 0, 0, // sequence number
            1, // number of tx outputs
            142, 211, 26, 75, 0, 0, 0, 0, // output value
            25, // scriptPubKey len
            118, 169, 20, 55, 56, 117, 33, 54, 199, 181, 222, 235, 189, 237, // scriptPubKey
            147, 39, 77, 105, 201, 13, 59, 32, 143, 136, 172,
            0, 0, 0, 0, // locktime
        ];
        CoinbaseTestHarness {
            cb_bytes: &non_segwit_cb_bytes,
            segwit: false,
            block_height_len: 3,
            expected_prefix_end: &[113, 104, 9],
            expected_suffix_start: &[0, 0, 0],
        }
        .test_coinbase_prefix_suffix();
    }
}
