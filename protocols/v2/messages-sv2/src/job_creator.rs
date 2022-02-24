use crate::utils::Id;
use binary_sv2::{B064K, U256};
use bitcoin::blockdata::script::Script;
use bitcoin::blockdata::transaction::{OutPoint, Transaction, TxIn, TxOut};
use bitcoin::consensus::encode::Encodable;
pub use bitcoin::secp256k1::SecretKey;
pub use bitcoin::util::ecdsa::{PrivateKey, PublicKey};
use mining_sv2::{NewExtendedMiningJob, Target};
use std::convert::TryInto;
use template_distribution_sv2::{CoinbaseOutputDataSize, NewTemplate, SetNewPrevHash};

/// Used by pool one for each group channel
/// extended and standard channel not supported
pub struct JobCreator {
    group_channel_id: u32,
    job_ids: Id,
    version_rolling_allowed: bool,
}

impl JobCreator {
    pub fn new_extended_job(
        &mut self,
        new_template: &mut NewTemplate,
        coinbase_outputs: &Vec<TxOut>,
    ) -> NewExtendedMiningJob {
        // TODO not supported yet
        if new_template.coinbase_tx_outputs_count != 0 {
            panic!("node provided outputs not supported yet")
        }
        let script_prefix = new_template.coinbase_prefix.to_vec();
        let script_prefix_len = script_prefix.len();
        if Self::extranonce_len(&script_prefix) != 100 {
            panic!("")
        }
        let coinbase = self.coinbase(
            script_prefix,
            new_template
                .coinbase_tx_version
                .try_into()
                .expect("invalid version"),
            new_template.coinbase_tx_locktime,
            new_template.coinbase_tx_input_sequence,
            coinbase_outputs,
        );
        let new_extended_mining_job = NewExtendedMiningJob {
            channel_id: self.group_channel_id,
            job_id: self.job_ids.next(),
            future_job: new_template.future_template,
            version: new_template.version,
            version_rolling_allowed: self.version_rolling_allowed,
            merkle_path: new_template.merkle_path.clone().into_static(),
            coinbase_tx_prefix: Self::coinbase_tx_prefix(&coinbase, script_prefix_len),
            coinbase_tx_suffix: Self::coinbase_tx_suffix(&coinbase, script_prefix_len),
        };
        new_extended_mining_job
    }

    /// Return the len of the script without the block hight (bip34) part
    /// https://btcinformation.org/en/developer-reference#compactsize-unsigned-integers
    /// https://developer.bitcoin.org/reference/transactions.html?highlight=coinbase#coinbase-input-the-input-of-the-first-transaction-in-a-block
    /// coinbase_tx_input_script_prefix: extranonce prefix (script lenght + bip34 block height) provided by the node
    fn extranonce_len(coinbase_tx_input_script_prefix: &[u8]) -> u8 {
        let script_len = coinbase_tx_input_script_prefix[0];
        script_len - coinbase_tx_input_script_prefix.len() as u8 + 1
    }

    fn coinbase_tx_prefix(
        coinbase: &Transaction,
        coinbase_tx_input_script_prefix_byte_len: usize,
    ) -> B064K<'static> {
        let mut encoded = Vec::new();
        coinbase.consensus_encode(&mut encoded).unwrap();
        let tx_version_byte_len = 4;
        // add 1 cause the script header (len of script) is 1 byte
        encoded[0..tx_version_byte_len + coinbase_tx_input_script_prefix_byte_len + 1]
            .to_vec()
            .try_into()
            .unwrap()
    }

    fn coinbase_tx_suffix(
        coinbase: &Transaction,
        coinbase_tx_input_script_prefix_byte_len: usize,
    ) -> B064K<'static> {
        let mut encoded = Vec::new();
        coinbase.consensus_encode(&mut encoded).unwrap();
        let tx_version_byte_len = 4;
        // TODO check if it is correct
        let extranonce_byte_len = 100;
        // add 1 cause the script header (len of script) is 1 byte
        encoded[tx_version_byte_len
            + coinbase_tx_input_script_prefix_byte_len
            + extranonce_byte_len
            + 1..]
            .to_vec()
            .try_into()
            .unwrap()
    }

    fn coinbase_script(mut coinbase_tx_input_script_prefix: Vec<u8>) -> Script {
        let remaning_len = Self::extranonce_len(&coinbase_tx_input_script_prefix);
        coinbase_tx_input_script_prefix.append(&mut vec![0, remaning_len]);
        coinbase_tx_input_script_prefix
            .try_into()
            .expect("invalid script")
    }

    /// coinbase_tx_input_script_prefix: extranonce prefix (script lenght + bip34 block height) provided by the node
    /// TODO it assume that NewTemplate.coinbase_tx_outputs == 0
    fn coinbase(
        &self,
        coinbase_tx_input_script_prefix: Vec<u8>,
        version: i32,
        lock_time: u32,
        sequence: u32,
        coinbase_outputs: &Vec<TxOut>,
    ) -> Transaction {
        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: Self::coinbase_script(coinbase_tx_input_script_prefix),
            sequence,
            witness: vec![],
        };
        Transaction {
            version,
            lock_time,
            input: vec![tx_in],
            output: coinbase_outputs.clone(),
        }
    }
}

/// Used by pool add a JobCreator for each group channel
/// extended and standard channel not supported
pub struct JobsCreators {
    jobs_creators: Vec<JobCreator>,
    /// Computed by the pool
    coinbase_outputs: Vec<TxOut>,
    block_reward_staoshi: u64,
    pub_key: PublicKey,
}

impl JobsCreators {
    pub fn new(block_reward_staoshi: u64, pub_key: PublicKey) -> Self {
        Self {
            jobs_creators: vec![],
            coinbase_outputs: Self::new_outputs(block_reward_staoshi, pub_key),
            block_reward_staoshi,
            pub_key,
        }
    }

    pub fn new_outputs(block_reward_staoshi: u64, pub_key: PublicKey) -> Vec<TxOut> {
        let script_pubkey = Script::new_v0_wpkh(&pub_key.wpubkey_hash().unwrap());
        vec![TxOut {
            value: block_reward_staoshi,
            script_pubkey,
        }]
    }

    pub fn on_new_template(&mut self, template: &mut NewTemplate) -> Vec<NewExtendedMiningJob> {
        if template.coinbase_tx_value_remaining != self.block_reward_staoshi {
            self.block_reward_staoshi = template.coinbase_tx_value_remaining;
            self.coinbase_outputs =
                Self::new_outputs(template.coinbase_tx_value_remaining, self.pub_key);
        }
        let coinbase_output = self.coinbase_outputs.clone();
        self.jobs_creators
            .iter_mut()
            .map(|jc| jc.new_extended_job(template, &coinbase_output))
            .collect()
    }

    pub fn new_group_channel(&mut self, group_channel_id: u32, version_rolling_allowed: bool) {
        let jc = JobCreator {
            group_channel_id,
            job_ids: Id::new(),
            version_rolling_allowed,
        };
        self.jobs_creators.push(jc);
    }
}
