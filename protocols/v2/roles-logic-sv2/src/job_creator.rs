use crate::{utils::Id, Error};
use binary_sv2::B064K;
use bitcoin::{
    blockdata::{
        script::Script,
        transaction::{OutPoint, Transaction, TxIn, TxOut},
    },
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

const SCRIPT_PREFIX_LEN: usize = 4;
const PREV_OUT_LEN: usize = 38;

#[derive(Debug)]
pub struct JobsCreators {
    /// Computed by the pool
    coinbase_outputs: Vec<TxOut>,
    script_kind: ScriptKind,
    lasts_new_template: Vec<NewTemplate<'static>>,
    job_to_template_id: HashMap<u32, u64>,
    templte_to_job_id: HashMap<u64, u32>,
    ids: Id,
    last_target: mining_sv2::Target,
    extranonce_len: u8,
}
#[derive(Debug, Clone)]
pub enum ScriptKind {
    PayToPubKey(WPubkeyHash),
    PayToPubKeyHash(PubkeyHash),
    PayToScriptHash(ScriptHash),
    PayToWitnessPublicKeyHash(WPubkeyHash),
    PayToWitnessScriptHash(WScriptHash),
    Custom(Vec<u8>),
}

impl JobsCreators {
    pub fn new(block_reward_staoshi: u64, script_kind: ScriptKind, extranonce_len: u8) -> Self {
        Self {
            coinbase_outputs: vec![Self::new_output(block_reward_staoshi, script_kind.clone())],
            script_kind,
            lasts_new_template: Vec::new(),
            job_to_template_id: HashMap::new(),
            templte_to_job_id: HashMap::new(),
            ids: Id::new(),
            last_target: mining_sv2::Target::new(0, 0),
            extranonce_len,
        }
    }

    fn new_output(block_reward_staoshi: u64, script_kind: ScriptKind) -> TxOut {
        let script = match script_kind {
            ScriptKind::PayToPubKey(hash) => Script::new_v0_wpkh(&hash),
            ScriptKind::PayToPubKeyHash(hash) => Script::new_p2pkh(&hash),
            ScriptKind::PayToScriptHash(hash) => Script::new_p2sh(&hash),
            ScriptKind::PayToWitnessPublicKeyHash(hash) => Script::new_v0_wpkh(&hash),
            ScriptKind::PayToWitnessScriptHash(hash) => Script::new_v0_wsh(&hash),
            ScriptKind::Custom(script) => script.into(),
        };
        TxOut {
            value: block_reward_staoshi,
            script_pubkey: script,
        }
    }

    pub fn new_outputs(&self, block_reward_staoshi: u64) -> Vec<TxOut> {
        vec![Self::new_output(
            block_reward_staoshi,
            self.script_kind.clone(),
        )]
    }

    pub fn get_template_id_from_job(&self, job_id: u32) -> Option<u64> {
        self.job_to_template_id.get(&job_id).map(|x| x - 1)
    }

    pub fn on_new_template(
        &mut self,
        template: &mut NewTemplate,
        version_rolling_allowed: bool,
    ) -> Result<NewExtendedMiningJob<'static>, Error> {
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
            &self.coinbase_outputs,
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
    assert!(
        new_template.coinbase_tx_outputs_count == 0,
        "node provided outputs not supported yet"
    );
    let tx_version = new_template
        .coinbase_tx_version
        .try_into()
        .map_err(|_| Error::TxVersionTooBig)?;
    // Txs version lower or equal to 1 are not allowed in new blocks we need it only to test the
    // JobCreator against old bitcoin blocks
    #[cfg(not(test))]
    if tx_version <= 1 {
        return Err(Error::TxVersionTooLow);
    };
    let script_prefix = new_template.coinbase_prefix.to_vec();
    // Is ok to panic here cause condition will be always true when not in a test chain
    // (regtest ecc ecc)
    #[cfg(not(test))]
    assert!(
        script_prefix.len() > 3,
        "Bitcoin blockchain should be at least 16 block long"
    );

    // first byte is the push op code
    // TODO explain why we remove it!!
    #[allow(unused_mut)]
    #[cfg(not(test))]
    let bip34_bytes = script_prefix[1..4].to_vec();

    #[cfg(test)]
    let bip34_bytes: Vec<u8>;
    #[cfg(test)]
    if tx_version == 1 {
        bip34_bytes = vec![];
    } else {
        bip34_bytes = script_prefix[1..4].to_vec();
    }

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
        coinbase_tx_prefix: coinbase_tx_prefix(&coinbase, SCRIPT_PREFIX_LEN, tx_version)?,
        coinbase_tx_suffix: coinbase_tx_suffix(
            &coinbase,
            SCRIPT_PREFIX_LEN,
            extranonce_len,
            tx_version,
        )?,
    };

    debug!(
        "New extended mining job created: {:?}",
        new_extended_mining_job
    );
    Ok(new_extended_mining_job)
}

fn coinbase_tx_prefix(
    coinbase: &Transaction,
    #[cfg(test)] mut coinbase_tx_input_script_prefix_byte_len: usize,
    #[cfg(not(test))] coinbase_tx_input_script_prefix_byte_len: usize,
    _tx_version: i32,
) -> Result<B064K<'static>, Error> {
    // Txs version lower or equal to 1 are not allowed in new blocks we need it only to test the
    // JobCreator against old bitcoin blocks
    #[cfg(test)]
    if _tx_version == 1 {
        coinbase_tx_input_script_prefix_byte_len = 0;
    }
    let encoded = coinbase.serialize();
    // add 1 cause the script header (len of script) is 1 byte
    let r = encoded[0..SCRIPT_PREFIX_LEN + coinbase_tx_input_script_prefix_byte_len + PREV_OUT_LEN]
        .to_vec();
    r.try_into().map_err(Error::BinarySv2Error)
}

fn coinbase_tx_suffix(
    coinbase: &Transaction,
    coinbase_tx_input_script_prefix_byte_len: usize,
    extranonce_len: u8,
    _tx_version: i32,
) -> Result<B064K<'static>, Error> {
    #[allow(unused_mut)]
    let mut script_prefix_len = SCRIPT_PREFIX_LEN;
    #[cfg(test)]
    if _tx_version == 1 {
        script_prefix_len = 0;
    };
    let encoded = coinbase.serialize();
    let r = encoded[script_prefix_len
        + coinbase_tx_input_script_prefix_byte_len
        + PREV_OUT_LEN
        + (extranonce_len as usize)..]
        .to_vec();
    r.try_into().map_err(Error::BinarySv2Error)
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
    bip34_bytes.extend_from_slice(&vec![0; extranonce_len as usize]);
    let tx_in = TxIn {
        previous_output: OutPoint::null(),
        script_sig: bip34_bytes.into(),
        sequence,
        witness: vec![],
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
        coinbase_prefix.resize_with(255, || {
            cmp::min(
                u8::arbitrary(&mut coinbase_prefix_gen),
                max_num_for_script_prefix,
            )
        });
        let coinbase_prefix: binary_sv2::B0255 = coinbase_prefix.try_into().unwrap();

        let mut coinbase_tx_outputs_gen = Gen::new(32);
        let mut coinbase_tx_outputs_inner: vec::Vec<u8> = vec::Vec::new();
        coinbase_tx_outputs_inner.resize_with(32, || u8::arbitrary(&mut coinbase_tx_outputs_gen));
        let coinbase_tx_outputs_inner: binary_sv2::B064K =
            coinbase_tx_outputs_inner.try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::Seq064K<binary_sv2::B064K> =
            vec![coinbase_tx_outputs_inner].into();

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

    pub fn new_pub_key() -> ScriptKind {
        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();
        let pub_k = PublicKey::from_private_key(&secp, &priv_k);
        ScriptKind::PayToPubKey(pub_k.wpubkey_hash().unwrap())
    }

    // Test job_id_from_template
    #[test]
    fn test_job_id_from_template() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key(), 32);

        jobs_creators.new_outputs(5_000_000_000);

        //Create a template
        let mut template = template_from_gen(&mut Gen::new(255));

        let job = jobs_creators
            .on_new_template(template.borrow_mut(), false)
            .unwrap();

        assert_eq!(
            jobs_creators.get_template_id_from_job(job.job_id),
            Some(template.template_id)
        );

        // Assert returns non if no match
        assert_eq!(jobs_creators.get_template_id_from_job(70), None);
    }

    // Test reset new template
    #[test]
    fn test_reset_new_template() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key(), 32);

        assert_eq!(jobs_creators.lasts_new_template.len(), 0);

        //Create a template
        let mut template = template_from_gen(&mut Gen::new(255));
        let _ = jobs_creators.on_new_template(template.borrow_mut(), false);

        assert_eq!(jobs_creators.lasts_new_template.len(), 1);
        assert_eq!(jobs_creators.lasts_new_template[0], template);

        //Create a 2nd template
        let template2 = template_from_gen(&mut Gen::new(255));

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
    #[test]
    fn test_on_new_prev_hash() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key(), 32);

        //Create a template
        let mut template = template_from_gen(&mut Gen::new(255));
        let _ = jobs_creators.on_new_template(template.borrow_mut(), false);
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
}
