//! # Job Creator
//!
//! This module provides logic to create extended mining jobs given a template from
//! a template provider as well as logic to clean up old templates when new blocks are mined.
use crate::{
    utils::{Coinbase, Id},
    Error,
};
use mining_sv2::NewExtendedMiningJob;
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, convert::TryInto};
use stratum_common::bitcoin::{blockdata::transaction::TxOut, consensus::Decodable, Amount};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::debug;

#[derive(Debug)]
pub struct JobsCreators {
    lasts_new_template: Vec<NewTemplate<'static>>,
    job_to_template_id: HashMap<u32, u64, BuildNoHashHasher<u32>>,
    templte_to_job_id: HashMap<u64, u32, BuildNoHashHasher<u64>>,
    ids: Id,
    last_target: mining_sv2::Target,
    last_ntime: Option<u32>,
    extranonce_len: u8,
}

/// Transforms the byte array `coinbase_outputs` in a vector of TxOut
/// It assumes the data to be valid data and does not do any kind of check
pub fn tx_outputs_to_costum_scripts(tx_outputs: &[u8]) -> Vec<TxOut> {
    let mut txs = vec![];
    let mut cursor = 0;
    let mut txouts = &tx_outputs[cursor..];
    while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
        let len = match out.script_pubkey.len() {
            a @ 0..=252 => 8 + 1 + a,
            a @ 253..=10000 => 8 + 3 + a,
            _ => break,
        };
        cursor += len;
        txs.push(out)
    }
    txs
}

impl JobsCreators {
    /// Constructor
    pub fn new(extranonce_len: u8) -> Self {
        Self {
            lasts_new_template: Vec::new(),
            job_to_template_id: HashMap::with_hasher(BuildNoHashHasher::default()),
            templte_to_job_id: HashMap::with_hasher(BuildNoHashHasher::default()),
            ids: Id::new(),
            last_target: mining_sv2::Target::new(0, 0),
            last_ntime: None,
            extranonce_len,
        }
    }

    /// Get template id from job
    pub fn get_template_id_from_job(&self, job_id: u32) -> Option<u64> {
        self.job_to_template_id.get(&job_id).map(|x| x - 1)
    }

    /// Used to create new jobs when a new template arrives
    pub fn on_new_template(
        &mut self,
        template: &mut NewTemplate,
        version_rolling_allowed: bool,
        mut pool_coinbase_outputs: Vec<TxOut>,
        additional_coinbase_script_data_len: usize,
    ) -> Result<NewExtendedMiningJob<'static>, Error> {
        let server_tx_outputs = template.coinbase_tx_outputs.to_vec();
        let mut outputs = tx_outputs_to_costum_scripts(&server_tx_outputs);
        pool_coinbase_outputs.append(&mut outputs);

        // This is to make sure that 0 is never used, so we can use 0 for
        // set_new_prev_hashes that do not refer to any future job/template if needed
        // Then we will do the inverse (-1) where needed
        let template_id = template.template_id + 1;
        self.lasts_new_template.push(template.as_static());
        let next_job_id = self.ids.next();
        self.job_to_template_id.insert(next_job_id, template_id);
        self.templte_to_job_id.insert(template_id, next_job_id);
        new_extended_job(
            template,
            &mut pool_coinbase_outputs,
            additional_coinbase_script_data_len,
            next_job_id,
            version_rolling_allowed,
            self.extranonce_len,
            self.last_ntime,
        )
    }

    pub(crate) fn reset_new_templates(&mut self, template: Option<NewTemplate<'static>>) {
        match template {
            Some(t) => self.lasts_new_template = vec![t],
            None => self.lasts_new_template = vec![],
        }
    }

    /// When we get a new `SetNewPrevHash` we need to clear all the other templates and only
    /// keep the one that matches the template_id of the new prev hash. If none match then
    /// we clear all the saved templates.
    pub fn on_new_prev_hash(&mut self, prev_hash: &SetNewPrevHash<'static>) -> Option<u32> {
        self.last_target = prev_hash.target.clone().into();
        self.last_ntime = prev_hash.header_timestamp.into(); // set correct ntime
        let template: Vec<NewTemplate<'static>> = self
            .lasts_new_template
            .clone()
            .into_iter()
            .filter(|a| a.template_id == prev_hash.template_id)
            .collect();
        match template.len() {
            0 => {
                self.reset_new_templates(None);
                None
            }
            1 => {
                self.reset_new_templates(Some(template[0].clone()));

                self.templte_to_job_id
                    .get(&(prev_hash.template_id + 1))
                    .copied()
            }
            // TODO how many templates can we have at max
            _ => todo!("{:#?}", template.len()),
        }
    }

    /// Returns the latest mining target
    pub fn last_target(&self) -> mining_sv2::Target {
        self.last_target.clone()
    }
}

/// Converts custom job into extended job
pub fn extended_job_from_custom_job(
    referenced_job: &mining_sv2::SetCustomMiningJob,
    additional_coinbase_script_data_len: usize,
    extranonce_len: u8,
) -> Result<NewExtendedMiningJob<'static>, Error> {
    let mut outputs =
        tx_outputs_to_costum_scripts(referenced_job.coinbase_tx_outputs.clone().as_ref());
    let mut template = NewTemplate {
        template_id: 0,
        future_template: false,
        version: referenced_job.version,
        coinbase_tx_version: referenced_job.coinbase_tx_version,
        coinbase_prefix: referenced_job.coinbase_prefix.clone(),
        coinbase_tx_input_sequence: referenced_job.coinbase_tx_input_n_sequence,
        coinbase_tx_value_remaining: referenced_job.coinbase_tx_value_remaining,
        coinbase_tx_outputs_count: outputs.len() as u32,
        coinbase_tx_outputs: referenced_job.coinbase_tx_outputs.clone(),
        coinbase_tx_locktime: referenced_job.coinbase_tx_locktime,
        merkle_path: referenced_job.merkle_path.clone(),
    };
    new_extended_job(
        &mut template,
        &mut outputs,
        additional_coinbase_script_data_len,
        0,
        true,
        extranonce_len,
        Some(referenced_job.min_ntime),
    )
}

// Returns an extended job given the provided template from the Template Provider and other
// Pool role related fields.
//
// Pool related arguments:
//
// * `coinbase_outputs`: coinbase output transactions specified by the pool.
// * `job_id`: incremented job identifier specified by the pool.
// * `version_rolling_allowed`: boolean specified by the channel.
// * `extranonce_len`: extranonce length specified by the channel.
fn new_extended_job(
    new_template: &mut NewTemplate,
    coinbase_outputs: &mut [TxOut],
    additional_coinbase_script_data_len: usize,
    job_id: u32,
    version_rolling_allowed: bool,
    extranonce_len: u8,
    ntime: Option<u32>,
) -> Result<NewExtendedMiningJob<'static>, Error> {
    coinbase_outputs[0].value = match new_template.coinbase_tx_value_remaining.checked_mul(1) {
        //check that value_remaining is updated by TP
        Some(result) => Amount::from_sat(result),
        None => return Err(Error::ValueRemainingNotUpdated),
    };
    let tx_version = new_template
        .coinbase_tx_version
        .try_into()
        .map_err(|_| Error::TxVersionTooBig)?;

    let script_sig_prefix = new_template.coinbase_prefix.to_vec();

    let coinbase = Coinbase::new(
        script_sig_prefix,
        tx_version,
        new_template.coinbase_tx_locktime,
        new_template.coinbase_tx_input_sequence,
        coinbase_outputs.to_vec(),
        additional_coinbase_script_data_len,
        extranonce_len,
    );

    let min_ntime = binary_sv2::Sv2Option::new(if new_template.future_template {
        None
    } else {
        ntime
    });

    let new_extended_mining_job: NewExtendedMiningJob<'static> = NewExtendedMiningJob {
        channel_id: 0,
        job_id,
        min_ntime,
        version: new_template.version,
        version_rolling_allowed,
        merkle_path: new_template.merkle_path.clone().into_static(),
        coinbase_tx_prefix: coinbase.coinbase_tx_prefix()?,
        coinbase_tx_suffix: coinbase.coinbase_tx_suffix()?,
    };

    debug!(
        "New extended mining job created: {:?}",
        new_extended_mining_job
    );
    Ok(new_extended_mining_job)
}

// Test
#[cfg(test)]

pub mod tests {
    use super::*;
    use crate::utils::merkle_root_from_path;
    #[cfg(feature = "prop_test")]
    use binary_sv2::u256_from_int;
    use quickcheck::{Arbitrary, Gen};
    use std::{cmp, vec};
    use stratum_common::bitcoin::{blockdata::transaction::Transaction, consensus};

    #[cfg(feature = "prop_test")]
    use std::borrow::BorrowMut;

    use stratum_common::bitcoin::{
        consensus::Encodable, secp256k1::Secp256k1, Network, PrivateKey, PublicKey,
    };

    pub fn template_from_gen(g: &mut Gen) -> NewTemplate<'static> {
        let mut coinbase_prefix_gen = Gen::new(255);
        let mut coinbase_prefix: vec::Vec<u8> = vec::Vec::new();

        let max_num_for_script_sig_prefix = 253;
        let prefix_len = cmp::min(u8::arbitrary(&mut coinbase_prefix_gen), 6);
        coinbase_prefix.push(prefix_len);
        coinbase_prefix.resize_with(prefix_len as usize + 2, || {
            cmp::min(
                u8::arbitrary(&mut coinbase_prefix_gen),
                max_num_for_script_sig_prefix,
            )
        });
        let coinbase_prefix: binary_sv2::B0255 = coinbase_prefix.try_into().unwrap();

        let mut coinbase_tx_outputs_gen = Gen::new(32);
        let mut coinbase_tx_outputs_inner: vec::Vec<u8> = vec::Vec::new();
        coinbase_tx_outputs_inner.resize_with(32, || u8::arbitrary(&mut coinbase_tx_outputs_gen));
        let coinbase_tx_outputs: binary_sv2::B064K = coinbase_tx_outputs_inner.try_into().unwrap();

        let mut merkle_path_inner_gen = Gen::new(32);
        let mut merkle_path_inner: vec::Vec<u8> = vec::Vec::new();
        merkle_path_inner.resize_with(32, || u8::arbitrary(&mut merkle_path_inner_gen));
        let merkle_path_inner: binary_sv2::U256 = merkle_path_inner.try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_inner].into();

        NewTemplate {
            template_id: u64::arbitrary(g),
            future_template: bool::arbitrary(g),
            version: u32::arbitrary(g),
            coinbase_tx_version: 2,
            coinbase_prefix,
            coinbase_tx_input_sequence: u32::arbitrary(g),
            coinbase_tx_value_remaining: u64::arbitrary(g),
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs,
            coinbase_tx_locktime: u32::arbitrary(g),
            merkle_path,
        }
    }

    const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
    const NETWORK: Network = Network::Testnet;

    #[cfg(feature = "prop_test")]
    const BLOCK_REWARD: u64 = 625_000_000_000;

    pub fn new_pub_key() -> PublicKey {
        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();

        PublicKey::from_private_key(&secp, &priv_k)
    }

    #[cfg(feature = "prop_test")]
    use stratum_common::bitcoin::ScriptBuf;

    // Test job_id_from_template
    #[cfg(feature = "prop_test")]
    #[quickcheck_macros::quickcheck]
    fn test_job_id_from_template(mut template: NewTemplate<'static>) {
        let mut prefix = template.coinbase_prefix.to_vec();
        if prefix.len() > 0 {
            let len = u8::min(prefix[0], 6);
            prefix[0] = len;
            prefix.resize(len as usize + 2, 0);
            template.coinbase_prefix = prefix.try_into().unwrap();
        };
        let out = TxOut {
            value: Amount::from_sat(BLOCK_REWARD),
            script_pubkey: ScriptBuf::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        let job = jobs_creators
            .on_new_template(template.borrow_mut(), false, vec![out], 0)
            .unwrap();

        assert_eq!(
            jobs_creators.get_template_id_from_job(job.job_id),
            Some(template.template_id)
        );

        // Assert returns non if no match
        assert_eq!(jobs_creators.get_template_id_from_job(70), None);
    }

    // Test reset new template
    #[cfg(feature = "prop_test")]
    #[quickcheck_macros::quickcheck]
    fn test_reset_new_template(mut template: NewTemplate<'static>) {
        let out = TxOut {
            value: Amount::from_sat(BLOCK_REWARD),
            script_pubkey: ScriptBuf::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        assert_eq!(jobs_creators.lasts_new_template.len(), 0);

        let _ = jobs_creators.on_new_template(template.borrow_mut(), false, vec![out], 0);

        assert_eq!(jobs_creators.lasts_new_template.len(), 1);
        assert_eq!(jobs_creators.lasts_new_template[0], template);

        //Create a 2nd template
        let mut template2 = template_from_gen(&mut Gen::new(255));
        template2.template_id = template.template_id.checked_sub(1).unwrap_or(0);

        // Reset new template
        jobs_creators.reset_new_templates(Some(template2.clone()));

        // Should be pointing at new template
        assert_eq!(jobs_creators.lasts_new_template.len(), 1);
        assert_eq!(jobs_creators.lasts_new_template[0], template2);

        // Reset new template
        jobs_creators.reset_new_templates(None);

        // Should be pointing at new template
        assert_eq!(jobs_creators.lasts_new_template.len(), 0);
    }

    // Test on_new_prev_hash
    #[cfg(feature = "prop_test")]
    #[quickcheck_macros::quickcheck]
    fn test_on_new_prev_hash(mut template: NewTemplate<'static>) {
        let out = TxOut {
            value: Amount::from_sat(BLOCK_REWARD),
            script_pubkey: ScriptBuf::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        //Create a template
        let _ = jobs_creators.on_new_template(template.borrow_mut(), false, vec![out], 0);
        let test_id = template.template_id;

        // Create a SetNewPrevHash with matching template_id
        let prev_hash = SetNewPrevHash {
            template_id: test_id,
            prev_hash: u256_from_int(45_u32),
            header_timestamp: 0,
            n_bits: 0,
            target: ([0_u8; 32]).try_into().unwrap(),
        };

        jobs_creators.on_new_prev_hash(&prev_hash);

        //Validate that we still have the same template loaded as there were matching templateIds
        assert_eq!(jobs_creators.lasts_new_template.len(), 1);
        assert_eq!(jobs_creators.lasts_new_template[0], template);

        // Create a SetNewPrevHash with matching template_id
        let test_id_2 = test_id.wrapping_add(1);
        let prev_hash2 = SetNewPrevHash {
            template_id: test_id_2,
            prev_hash: u256_from_int(45_u32),
            header_timestamp: 0,
            n_bits: 0,
            target: ([0_u8; 32]).try_into().unwrap(),
        };

        jobs_creators.on_new_prev_hash(&prev_hash2);

        //Validate that templates were cleared as we got a new templateId in setNewPrevHash
        assert_eq!(jobs_creators.lasts_new_template.len(), 0);
    }

    #[quickcheck_macros::quickcheck]
    fn it_parse_valid_tx_outs(
        mut hash1: Vec<u8>,
        mut hash2: Vec<u8>,
        value1: u64,
        value2: u64,
        size1: u8,
        size2: u8,
    ) {
        hash1.resize(size1 as usize + 2, 0);
        hash2.resize(size2 as usize + 2, 0);
        let tx1 = TxOut {
            value: Amount::from_sat(value1),
            script_pubkey: hash1.into(),
        };
        let tx2 = TxOut {
            value: Amount::from_sat(value2),
            script_pubkey: hash2.into(),
        };
        let mut encoded1 = vec![];
        let mut encoded2 = vec![];
        tx1.consensus_encode(&mut encoded1).unwrap();
        tx2.consensus_encode(&mut encoded2).unwrap();
        let mut encoded = vec![];
        encoded.append(&mut encoded1.clone());
        encoded.append(&mut encoded2.clone());
        let outs = tx_outputs_to_costum_scripts(&encoded[..]);
        assert!(outs[0] == tx1);
        assert!(outs[1] == tx2);
    }

    // test that witness stripped tx id matches that of the txid of the coinbase
    #[test]
    fn stripped_tx_id() {
        let encoded: &[u8] = &[
            2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 36, 2, 107, 22, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255,
            255, 255, 2, 0, 0, 0, 0, 0, 0, 0, 0, 67, 65, 4, 70, 109, 127, 202, 229, 99, 229, 203,
            9, 160, 209, 135, 11, 181, 128, 52, 72, 4, 97, 120, 121, 161, 73, 73, 207, 34, 40, 95,
            27, 174, 63, 39, 103, 40, 23, 108, 60, 100, 49, 248, 238, 218, 69, 56, 220, 55, 200,
            101, 226, 120, 79, 58, 158, 119, 208, 68, 243, 62, 64, 119, 151, 225, 39, 138, 172, 0,
            0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
            253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
            235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let tx: Transaction = consensus::deserialize(encoded).unwrap();

        // Calculate script_sig_prefix_len by examining the actual script_sig length
        // and subtracting the extranonce length (32 in this case)
        let extranonce_len = 32;
        let script_sig_len = tx.input[0].script_sig.len();
        let script_sig_prefix_len = script_sig_len - extranonce_len;

        let coinbase = Coinbase {
            tx,
            script_sig_prefix_len,
        };

        let prefix = coinbase.coinbase_tx_prefix().unwrap().to_vec();
        let suffix = coinbase.coinbase_tx_suffix().unwrap().to_vec();
        let extranonce = &[0_u8; 32];
        let path: &[binary_sv2::U256] = &[];
        let stripped_merkle_root =
            merkle_root_from_path(&prefix[..], &suffix[..], extranonce, path).unwrap();

        let og_merkle_root = coinbase.tx.compute_txid();
        let og_merkle_root_bytes = og_merkle_root.as_raw_hash();

        assert_eq!(
            stripped_merkle_root.as_slice(),
            &og_merkle_root_bytes[..],
            "stripped tx hash is not the same as bitcoin crate"
        );
    }
    #[test]
    fn stripped_tx_id_braiins_example() {
        let mut encoded = vec![];
        let coinbase_prefix = &[
            1_u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 75, 3, 176, 235, 11, 250, 190, 109, 109,
            50, 247, 22, 140, 225, 176, 1, 231, 78, 225, 50, 226, 181, 165, 55, 145, 137, 154, 46,
            9, 44, 65, 72, 231, 173, 111, 131, 26, 81, 223, 179, 225, 1, 0, 0, 0, 0, 0, 0, 0,
        ];
        let coinbase_suffix = &[
            245_u8, 192, 42, 69, 19, 47, 115, 108, 117, 115, 104, 47, 0, 0, 0, 0, 3, 78, 213, 148,
            39, 0, 0, 0, 0, 25, 118, 169, 20, 124, 21, 78, 209, 220, 89, 96, 158, 61, 38, 171, 178,
            223, 46, 163, 213, 135, 205, 140, 65, 136, 172, 0, 0, 0, 0, 0, 0, 0, 0, 44, 106, 76,
            41, 82, 83, 75, 66, 76, 79, 67, 75, 58, 214, 9, 239, 96, 221, 25, 108, 87, 155, 50, 55,
            47, 91, 115, 172, 168, 0, 12, 86, 195, 26, 241, 10, 22, 190, 151, 254, 24, 0, 78, 106,
            26, 0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 103, 66, 68, 105, 2, 55,
            65, 241, 216, 46, 82, 223, 150, 0, 97, 103, 2, 82, 186, 233, 145, 90, 210, 231, 35,
            100, 107, 52, 171, 233, 50, 200, 0, 0, 0, 0,
        ];
        let extranonce = [0_u8; 15]; // braiins pool requires 15 bytes for extranonce
        encoded.extend_from_slice(coinbase_prefix);
        let mut encoded_clone = encoded.clone();
        encoded_clone.extend_from_slice(&extranonce);
        encoded_clone.extend_from_slice(coinbase_suffix);
        let parsed_tx: Transaction = consensus::deserialize(&encoded_clone).unwrap();

        let coinbase = Coinbase {
            tx: parsed_tx.clone(),
            script_sig_prefix_len: 60,
        };

        let prefix = coinbase.coinbase_tx_prefix().unwrap().to_vec();
        let suffix = coinbase.coinbase_tx_suffix().unwrap().to_vec();

        let mut encoded_b = vec![];
        encoded_b.extend_from_slice(&prefix);
        encoded_b.extend_from_slice(&extranonce[..]);
        encoded_b.extend_from_slice(&suffix);

        // parse one last time to ensure that Coinbase::coinbase_tx_prefix and
        // Coinbase::coinbase_tx_suffix functions are sane
        let _parsed_tx_b: Transaction = consensus::deserialize(&encoded_b).unwrap();
    }
}
