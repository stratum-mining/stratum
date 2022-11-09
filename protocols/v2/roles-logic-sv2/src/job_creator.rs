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
    secp256k1::SecretKey,
    util::ecdsa::{PrivateKey, PublicKey},
};
use mining_sv2::NewExtendedMiningJob;
use std::{collections::HashMap, convert::TryInto};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use tracing::debug;

const SCRIPT_PREFIX_LEN: usize = 4;
const PREV_OUT_LEN: usize = 38;
const EXTRANONCE_LEN: usize = 32;

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
        assert!(
            new_template.coinbase_tx_outputs_count == 0,
            "node provided outputs not supported yet"
        );
        let script_prefix = new_template.coinbase_prefix.to_vec();
        // Is ok to panic here cause condition will be always true when not in a test chain
        // (regtest ecc ecc)
        assert!(
            script_prefix.len() > 3,
            "Bitcoin blockchain should be at least 16 block long"
        );
        let bip34_len = script_prefix[1] as usize;
        let bip34_bytes = script_prefix[1..2 + bip34_len].to_vec();

        let coinbase = self.coinbase(
            bip34_bytes,
            new_template
                .coinbase_tx_version
                .try_into()
                .expect("invalid version"),
            new_template.coinbase_tx_locktime,
            new_template.coinbase_tx_input_sequence,
            coinbase_outputs,
        );
        let new_extended_mining_job: NewExtendedMiningJob<'static> = NewExtendedMiningJob {
            channel_id: self.group_channel_id,
            job_id: self.job_ids.next(),
            future_job: new_template.future_template,
            version: new_template.version,
            version_rolling_allowed: self.version_rolling_allowed,
            merkle_path: new_template.merkle_path.clone().into_static(),
            coinbase_tx_prefix: Self::coinbase_tx_prefix(&coinbase, SCRIPT_PREFIX_LEN)?,
            coinbase_tx_suffix: Self::coinbase_tx_suffix(&coinbase, SCRIPT_PREFIX_LEN)?,
        };
        self.template_id_to_job_id
            .insert(new_template.template_id, new_extended_mining_job.job_id);

        debug!(
            "New extended mining job created: {:?}",
            new_extended_mining_job
        );
        Ok(new_extended_mining_job)
    }

    fn get_job_id(&self, template_id: u64) -> Option<u32> {
        self.template_id_to_job_id.get(&template_id).copied()
    }

    fn coinbase_tx_prefix(
        coinbase: &Transaction,
        coinbase_tx_input_script_prefix_byte_len: usize,
    ) -> Result<B064K<'static>, Error> {
        let encoded = coinbase.serialize();
        // add 1 cause the script header (len of script) is 1 byte
        let r = encoded
            [0..SCRIPT_PREFIX_LEN + coinbase_tx_input_script_prefix_byte_len + PREV_OUT_LEN]
            .to_vec();
        r.try_into().map_err(Error::BinarySv2Error)
    }

    fn coinbase_tx_suffix(
        coinbase: &Transaction,
        coinbase_tx_input_script_prefix_byte_len: usize,
    ) -> Result<B064K<'static>, Error> {
        let encoded = coinbase.serialize();
        let r = encoded[SCRIPT_PREFIX_LEN
            + coinbase_tx_input_script_prefix_byte_len
            + PREV_OUT_LEN
            + EXTRANONCE_LEN..]
            .to_vec();
        r.try_into().map_err(Error::BinarySv2Error)
    }

    /// coinbase_tx_input_script_prefix: extranonce prefix (script lenght + bip34 block height) provided by the node
    /// It assume that NewTemplate.coinbase_tx_outputs == 0
    fn coinbase(
        &self,
        mut bip34_bytes: Vec<u8>,
        version: i32,
        lock_time: u32,
        sequence: u32,
        coinbase_outputs: &[TxOut],
    ) -> Transaction {
        bip34_bytes.extend_from_slice(&[0; EXTRANONCE_LEN]);
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

    /// When we get a new SetNewPrevHash we need to clear all the other templates and only
    /// keep the one that matches the template_id of the new prev hash. If none match then
    /// we clear all the saved templates.
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
        debug!("new_group_channel created: {}", group_channel_id);
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

// Test
#[cfg(test)]

mod tests {
    use super::*;
    use binary_sv2::u256_from_int;
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::Network;
    use quickcheck::{Arbitrary, Gen};
    use std::borrow::BorrowMut;
    use std::vec;

    pub fn from_gen(g: &mut Gen, id: u64) -> NewTemplate<'static> {
        let mut coinbase_prefix_gen = Gen::new(255);
        let mut coinbase_prefix: vec::Vec<u8> = vec::Vec::new();
        coinbase_prefix.resize_with(255, || u8::arbitrary(&mut coinbase_prefix_gen));
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
            template_id: id,
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

    fn new_pub_key() -> PublicKey {
        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();
        PublicKey::from_private_key(&secp, &priv_k)
    }

    // Test job_id_from_template
    #[test]
    fn test_job_id_from_template() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key()).unwrap();

        jobs_creators.new_group_channel(1, true).unwrap();

        let test_id: u64 = 20;
        //Create a template
        let mut template = from_gen(&mut Gen::new(255), test_id);
        let _jobs = jobs_creators.on_new_template(template.borrow_mut());

        assert_eq!(jobs_creators.job_id_from_template(test_id, 1), Some(1));

        // Assert returns non if no match
        assert_eq!(jobs_creators.job_id_from_template(test_id, 2), None);
    }

    // Test new_group_channel
    #[test]
    fn test_new_group_channel() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key()).unwrap();

        jobs_creators.new_group_channel(1, true).unwrap();

        let test_id: u64 = 20;
        //Create a template
        let mut template = from_gen(&mut Gen::new(255), test_id);
        let _jobs = jobs_creators.on_new_template(template.borrow_mut());

        let res = jobs_creators.new_group_channel(2, true).unwrap();
        assert_eq!(res.len(), 1);

        // Assert there are now 2 job creators - one for each group
        assert_eq!(jobs_creators.jobs_creators.len(), 2);
    }

    // Test on_new_template
    #[test]
    fn test_on_new_template() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key()).unwrap();

        let channel_id = 1;
        jobs_creators.new_group_channel(channel_id, true).unwrap();

        let test_id: u64 = 20;
        //Create a template
        let mut template = from_gen(&mut Gen::new(255), test_id);
        let jobs = jobs_creators.on_new_template(template.borrow_mut()).unwrap();
        
        assert_eq!(jobs[&channel_id].job_id, 1);
        assert_eq!(jobs[&channel_id].channel_id, 1);
        assert_eq!(jobs[&channel_id].future_job, template.future_template);
        assert_eq!(jobs[&channel_id].version, template.version);
        assert_eq!(jobs[&channel_id].version_rolling_allowed, true);
        assert_eq!(jobs[&channel_id].merkle_path, template.merkle_path);

    }

    // Test reset new template
    #[test]
    fn test_reset_new_template() {
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key()).unwrap();

        assert_eq!(jobs_creators.lasts_new_template.len(), 0);

        jobs_creators.new_group_channel(1, true).unwrap();

        let test_id: u64 = 22;
        //Create a template
        let mut template = from_gen(&mut Gen::new(255), test_id);
        let _ = jobs_creators.on_new_template(template.borrow_mut());

        assert_eq!(jobs_creators.lasts_new_template.len(), 1);
        assert_eq!(jobs_creators.lasts_new_template[0], template);

        let test_id2: u64 = 21;
        //Create a 2nd template
        let template2 = from_gen(&mut Gen::new(255), test_id2);

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
        let mut jobs_creators = JobsCreators::new(BLOCK_REWARD, new_pub_key()).unwrap();

        jobs_creators.new_group_channel(1, true).unwrap();

        let test_id: u64 = 23;
        //Create a template
        let mut template = from_gen(&mut Gen::new(255), test_id);
        let _ = jobs_creators.on_new_template(template.borrow_mut());

        // Assert returns non if no match
        assert_eq!(jobs_creators.job_id_from_template(test_id, 2), None);

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

        //let mut u256 = [0_u8; 32];

        // Create a SetNewPrevHash with matching template_id
        let prev_hash2 = SetNewPrevHash {
            template_id: test_id + 1,
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
