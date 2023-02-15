use crate::{utils::Id, Error};
use binary_sv2::B064K;
use bitcoin::{
    blockdata::transaction::{OutPoint, Transaction, TxIn, TxOut},
    util::psbt::serialize::Serialize,
};
pub use bitcoin::{
    hash_types::{PubkeyHash, ScriptHash, WPubkeyHash, WScriptHash},
    secp256k1::SecretKey,
    util::ecdsa::PrivateKey,
};
use mining_sv2::NewExtendedMiningJob;
use std::{collections::HashMap, convert::TryInto};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::debug;

#[derive(Debug)]
pub struct JobsCreators {
    lasts_new_template: Vec<NewTemplate<'static>>,
    job_to_template_id: HashMap<u32, u64>,
    templte_to_job_id: HashMap<u64, u32>,
    ids: Id,
    last_target: mining_sv2::Target,
    extranonce_len: u8,
}

use bitcoin::consensus::Decodable;

/// Transform the byte array `coinbase_outputs` in a vector of TxOut
/// It assume the data to be valid data and do not do any kind of check
pub fn tx_outputs_to_costum_scripts(tx_outputs: &[u8]) -> Vec<TxOut> {
    let mut txs = vec![];
    let mut cursor = 0;
    while let Ok(out) = TxOut::consensus_decode(&tx_outputs[cursor..]) {
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
    pub fn new(extranonce_len: u8) -> Self {
        Self {
            lasts_new_template: Vec::new(),
            job_to_template_id: HashMap::new(),
            templte_to_job_id: HashMap::new(),
            ids: Id::new(),
            last_target: mining_sv2::Target::new(0, 0),
            extranonce_len,
        }
    }

    pub fn get_template_id_from_job(&self, job_id: u32) -> Option<u64> {
        self.job_to_template_id.get(&job_id).map(|x| x - 1)
    }

    pub fn on_new_template(
        &mut self,
        template: &mut NewTemplate,
        version_rolling_allowed: bool,
        mut pool_coinbase_outputs: Vec<TxOut>,
    ) -> Result<NewExtendedMiningJob<'static>, Error> {
        let server_tx_outputs = template.coinbase_tx_outputs.to_vec();
        let mut outputs = tx_outputs_to_costum_scripts(&server_tx_outputs);
        pool_coinbase_outputs.append(&mut outputs);
        //self.coinbase_outputs = pool_coinbase_outputs;

        // This is to make sure that 0 is never used that is usefull so we can use 0 for
        // set_new_prev_hash that do not refer to any future job/template if needed
        // Then we will do the inverse (-1) where needed
        let template_id = template.template_id + 1;
        self.lasts_new_template.push(template.as_static());
        let next_job_id = self.ids.next();
        self.job_to_template_id.insert(next_job_id, template_id);
        self.templte_to_job_id.insert(template_id, next_job_id);
        new_extended_job(
            template,
            &pool_coinbase_outputs,
            next_job_id,
            version_rolling_allowed,
            self.extranonce_len,
        )
    }

    pub(crate) fn reset_new_templates(&mut self, template: Option<NewTemplate<'static>>) {
        match template {
            Some(t) => self.lasts_new_template = vec![t],
            None => self.lasts_new_template = vec![],
        }
    }

    /// When we get a new SetNewPrevHash we need to clear all the other templates and only
    /// keep the one that matches the template_id of the new prev hash. If none match then
    /// we clear all the saved templates.
    pub fn on_new_prev_hash(&mut self, prev_hash: &SetNewPrevHash<'static>) -> Option<u32> {
        self.last_target = prev_hash.target.clone().into();
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

                // unwrap is safe cause we always poulate the map on_new_template
                Some(
                    *self
                        .templte_to_job_id
                        .get(&(prev_hash.template_id + 1))
                        .unwrap(),
                )
            }
            // TODO how many templates can we have at max
            _ => todo!("{:#?}", template.len()),
        }
    }

    pub fn last_target(&self) -> mining_sv2::Target {
        self.last_target.clone()
    }
}

fn new_extended_job(
    new_template: &mut NewTemplate,
    coinbase_outputs: &[TxOut],
    job_id: u32,
    version_rolling_allowed: bool,
    extranonce_len: u8,
) -> Result<NewExtendedMiningJob<'static>, Error> {
    let tx_version = new_template
        .coinbase_tx_version
        .try_into()
        .map_err(|_| Error::TxVersionTooBig)?;

    let bip34_bytes = get_bip_34_bytes(new_template, tx_version)?;
    let script_prefix_len = bip34_bytes.len();

    let coinbase = coinbase(
        bip34_bytes,
        tx_version,
        new_template.coinbase_tx_locktime,
        new_template.coinbase_tx_input_sequence,
        coinbase_outputs,
        extranonce_len,
    );

    let new_extended_mining_job: NewExtendedMiningJob<'static> = NewExtendedMiningJob {
        channel_id: 0,
        job_id,
        future_job: new_template.future_template,
        version: new_template.version,
        version_rolling_allowed,
        merkle_path: new_template.merkle_path.clone().into_static(),
        coinbase_tx_prefix: coinbase_tx_prefix(&coinbase, script_prefix_len)?,
        coinbase_tx_suffix: coinbase_tx_suffix(&coinbase, extranonce_len, script_prefix_len)?,
    };

    debug!(
        "New extended mining job created: {:?}",
        new_extended_mining_job
    );
    Ok(new_extended_mining_job)
}

fn coinbase_tx_prefix(
    coinbase: &Transaction,
    script_prefix_len: usize,
) -> Result<B064K<'static>, Error> {
    let encoded = coinbase.serialize();
    // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the 0
    // witness
    let segwit_bytes = match script_prefix_len {
        0 => 0,
        _ => 2,
    };
    let index = 4    // tx version
        + segwit_bytes
        + 1  // number of inputs TODO can be also 3
        + 32 // prev OutPoint
        + 4  // index
        + 1  // bytes in script TODO can be also 3
        + script_prefix_len; // bip34_bytes
    let r = encoded[0..index].to_vec();
    r.try_into().map_err(Error::BinarySv2Error)
}

fn coinbase_tx_suffix(
    coinbase: &Transaction,
    extranonce_len: u8,
    script_prefix_len: usize,
) -> Result<B064K<'static>, Error> {
    let encoded = coinbase.serialize();
    // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the 0
    // witness
    let segwit_bytes = match script_prefix_len {
        0 => 0,
        _ => 2,
    };
    let r = encoded[4    // tx version
        + segwit_bytes
        + 1  // number of inputs TODO can be also 3
        + 32 // prev OutPoint
        + 4  // index
        + 1  // bytes in script TODO can be also 3
        + script_prefix_len  // bip34_bytes
        + (extranonce_len as usize)..]
        .to_vec();
    r.try_into().map_err(Error::BinarySv2Error)
}

// Just double check if received coinbase_prefix is the right one can be removed or used only for
// tests
fn get_bip_34_bytes(new_template: &NewTemplate, tx_version: i32) -> Result<Vec<u8>, Error> {
    #[cfg(test)]
    if tx_version == 1 {
        return Ok(vec![]);
    };

    let script_prefix = &new_template.coinbase_prefix.to_vec()[..];

    // Is ok to panic here cause condition will be always true when not in a test chain
    // (regtest ecc ecc)
    #[cfg(not(test))]
    assert!(
        script_prefix.len() > 2,
        "Bitcoin blockchain should be at least 16 block long"
    );

    // Txs version lower or equal to 1 are not allowed in new blocks we need it only to test the
    // JobCreator against old bitcoin blocks
    #[cfg(not(test))]
    if tx_version <= 1 {
        return Err(Error::TxVersionTooLow);
    };

    // add 1 cause 0 is push 1 2 is 1 is push 2 ecc ecc
    // add 1 cause in the len there is also the op code itself
    let bip34_len = script_prefix[0] as usize + 2;
    if bip34_len == script_prefix.len() {
        Ok(script_prefix[0..bip34_len].to_vec())
    } else {
        Err(Error::InvalidBip34Bytes(script_prefix.to_vec()))
    }
}

/// coinbase_tx_input_script_prefix: extranonce prefix (script lenght + bip34 block height) provided by the node
/// It assume that NewTemplate.coinbase_tx_outputs == 0
fn coinbase(
    mut bip34_bytes: Vec<u8>,
    version: i32,
    lock_time: u32,
    sequence: u32,
    coinbase_outputs: &[TxOut],
    extranonce_len: u8,
) -> Transaction {
    // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the 0
    // witness
    let witness = match bip34_bytes.len() {
        0 => vec![],
        _ => vec![vec![0; 32]],
    };
    bip34_bytes.extend_from_slice(&vec![0; extranonce_len as usize]);
    let tx_in = TxIn {
        previous_output: OutPoint::null(),
        script_sig: bip34_bytes.into(),
        sequence,
        witness,
    };
    Transaction {
        version,
        lock_time,
        input: vec![tx_in],
        output: coinbase_outputs.to_vec(),
    }
}

// Test
#[cfg(test)]

pub mod tests {
    use super::*;
    use binary_sv2::u256_from_int;
    use bitcoin::{secp256k1::Secp256k1, util::ecdsa::PublicKey, Network};
    use quickcheck::{Arbitrary, Gen};
    use std::{borrow::BorrowMut, cmp, vec};

    pub fn template_from_gen(g: &mut Gen) -> NewTemplate<'static> {
        let mut coinbase_prefix_gen = Gen::new(255);
        let mut coinbase_prefix: vec::Vec<u8> = vec::Vec::new();

        let max_num_for_script_prefix = 253;
        let prefix_len = cmp::min(u8::arbitrary(&mut coinbase_prefix_gen), 6);
        coinbase_prefix.push(prefix_len);
        coinbase_prefix.resize_with(prefix_len as usize + 2, || {
            cmp::min(
                u8::arbitrary(&mut coinbase_prefix_gen),
                max_num_for_script_prefix,
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

    const BLOCK_REWARD: u64 = 625_000_000_000;

    pub fn new_pub_key() -> PublicKey {
        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();
        let pub_k = PublicKey::from_private_key(&secp, &priv_k);
        pub_k
    }
    use bitcoin::Script;

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
            value: BLOCK_REWARD,
            script_pubkey: Script::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        let job = jobs_creators
            .on_new_template(template.borrow_mut(), false, vec![out])
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
            value: BLOCK_REWARD,
            script_pubkey: Script::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        assert_eq!(jobs_creators.lasts_new_template.len(), 0);

        let _ = jobs_creators.on_new_template(template.borrow_mut(), false, vec![out]);

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
            value: BLOCK_REWARD,
            script_pubkey: Script::new_p2pk(&new_pub_key()),
        };
        let mut jobs_creators = JobsCreators::new(32);

        //Create a template
        let _ = jobs_creators.on_new_template(template.borrow_mut(), false, vec![out]);
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

    use bitcoin::consensus::Encodable;

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
            value: value1,
            script_pubkey: hash1.into(),
        };
        let tx2 = TxOut {
            value: value2,
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
        assert!(&outs[0] == &tx1);
        assert!(outs[1] == tx2);
    }
}
