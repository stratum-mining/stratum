use crate::{
    job_management::chain_tip::ChainTip, template_distribution_sv2::NewTemplate,
    utils::Id as JobIdFactory,
};
use binary_sv2::{Seq0255, Sv2Option, B064K, U256};
use mining_sv2::{NewExtendedMiningJob, SetCustomMiningJob};
use std::convert::TryInto;
use stratum_common::bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    consensus::{serialize, Decodable},
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
    Amount, Sequence,
};

/// Encapsulates an extended mining job as:
/// - the `NewTemplate` message that generated it (if any, custom jobs have no known template)
/// - the coinbase reward outputs
/// - the `NewExtendedMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct ExtendedJob<'a> {
    template: Option<NewTemplate<'a>>, // custom jobs have no known template
    coinbase_reward_outputs: Vec<TxOut>,
    job_message: NewExtendedMiningJob<'a>,
}

impl<'a> ExtendedJob<'a> {
    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_template(&self) -> Option<&NewTemplate<'a>> {
        self.template.as_ref()
    }

    pub fn get_coinbase_reward_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_reward_outputs
    }

    pub fn get_coinbase_tx_prefix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_prefix
    }

    pub fn get_coinbase_tx_suffix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_suffix
    }

    pub fn get_job_message(&self) -> &NewExtendedMiningJob<'a> {
        &self.job_message
    }

    pub fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        &self.job_message.merkle_path
    }

    pub fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.min_ntime.clone()
    }

    pub fn get_version(&self) -> u32 {
        self.job_message.version
    }

    pub fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed
    }

    /// Activates the job, setting the `min_ntime` field of the `NewExtendedMiningJob` message.
    ///
    /// To be used while activating future jobs upon updating channel `ChainTip` state.
    pub fn activate(&mut self, min_ntime: u32) {
        self.job_message.min_ntime = Sv2Option::new(Some(min_ntime));
    }
}

/// A Factory for creating Extended Jobs.
///
/// Please note that there is no `StandardJobFactory` counterpart. Since every Standard Channel
/// belongs to some Group Channel, all Standard Channels keep track of Standard Jobs that are
/// actually conversions from the Extended Job created on the Group Channel level.
#[derive(Debug, Clone)]
pub struct ExtendedJobFactory {
    job_id_factory: JobIdFactory,
    rollable_extranonce_size: u16,
    version_rolling_allowed: bool,
}

impl ExtendedJobFactory {
    pub fn new(rollable_extranonce_size: u16, version_rolling_allowed: bool) -> Self {
        Self {
            job_id_factory: JobIdFactory::new(),
            rollable_extranonce_size,
            version_rolling_allowed,
        }
    }

    pub fn new_job<'a>(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix_len: usize,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<ExtendedJob<'a>, ExtendedJobFactoryError> {
        let job_id = self.job_id_factory.next();

        let version = template.version;

        let coinbase_tx_prefix = self.coinbase_tx_prefix(
            extranonce_prefix_len,
            template.clone(),
            coinbase_reward_outputs.clone(),
        )?;
        let coinbase_tx_suffix = self.coinbase_tx_suffix(
            extranonce_prefix_len,
            template.clone(),
            coinbase_reward_outputs.clone(),
        )?;
        let merkle_path = template.merkle_path.clone();

        let job_message = match template.future_template {
            true => NewExtendedMiningJob {
                channel_id,
                job_id,
                min_ntime: Sv2Option::new(None),
                version,
                version_rolling_allowed: self.version_rolling_allowed,
                merkle_path,
                coinbase_tx_prefix,
                coinbase_tx_suffix,
            },
            false => NewExtendedMiningJob {
                channel_id,
                job_id,
                min_ntime: Sv2Option::new(chain_tip.map(|c| c.min_ntime())),
                version,
                version_rolling_allowed: self.version_rolling_allowed,
                merkle_path,
                coinbase_tx_prefix,
                coinbase_tx_suffix,
            },
        };

        let job = ExtendedJob {
            template: Some(template),
            job_message,
            coinbase_reward_outputs,
        };

        Ok(job)
    }

    pub fn new_custom_job<'a>(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'a>,
        extranonce_prefix_len: usize,
    ) -> Result<ExtendedJob<'a>, ExtendedJobFactoryError> {
        // This method creates a new mining job from a SetCustomMiningJob message
        // Unlike regular jobs created from NewTemplate, custom jobs don't have an associated
        // template.
        // We need to extract the coinbase transaction parts and parse the
        // serialized outputs into the appropriate format for the ExtendedJob structure.

        let job_id = self.job_id_factory.next();

        let version = set_custom_mining_job.version;

        let coinbase_tx_prefix =
            self.custom_coinbase_tx_prefix(extranonce_prefix_len, set_custom_mining_job.clone())?;
        let coinbase_tx_suffix =
            self.custom_coinbase_tx_suffix(extranonce_prefix_len, set_custom_mining_job.clone())?;
        let merkle_path = set_custom_mining_job.merkle_path.clone().into_static();

        let job_message = NewExtendedMiningJob {
            channel_id: set_custom_mining_job.channel_id,
            job_id,
            min_ntime: Sv2Option::new(Some(set_custom_mining_job.min_ntime)),
            version,
            version_rolling_allowed: self.version_rolling_allowed,
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            merkle_path,
        };

        // Parse the serialized outputs into a Vec<TxOut>
        let mut coinbase_reward_outputs: Vec<TxOut> = vec![];
        let serialized_outputs = set_custom_mining_job
            .coinbase_tx_outputs
            .inner_as_ref()
            .to_vec();

        // The serialized outputs are in Bitcoin consensus format
        // We need to parse them one by one, keeping track of cursor position
        let mut cursor = 0;
        let mut txouts = &serialized_outputs[cursor..];

        // Iteratively decode each TxOut until we can't decode any more
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            // Calculate the size of this TxOut based on its script_pubkey length
            // 8 bytes for value + variable bytes for script_pubkey length
            // For small scripts (0-252 bytes): 1 byte length prefix
            // For medium scripts (253-1000000 bytes): 3 byte length prefix (1 marker + 2 byte
            // length)
            let len = match out.script_pubkey.len() {
                a @ 0..=252 => 8 + 1 + a,       // 8 (value) + 1 (compact size) + script_len
                a @ 253..=1000000 => 8 + 3 + a, // 8 (value) + 3 (compact size) + script_len
                _ => break,                     // Unreasonably large script, likely an error
            };

            // Move the cursor forward by the size of this TxOut
            cursor += len;
            coinbase_reward_outputs.push(out);
        }

        let job = ExtendedJob {
            template: None,
            job_message,
            coinbase_reward_outputs,
        };

        Ok(job)
    }
}

// impl block with private methods
impl ExtendedJobFactory {
    // build a coinbase transaction from a SetCustomMiningJob
    // this is only used to extract coinbase_tx_prefix and coinbase_tx_suffix from the custom
    // coinbase
    fn custom_coinbase(
        &self,
        extranonce_prefix_len: usize,
        m: SetCustomMiningJob<'_>,
    ) -> Result<Transaction, ExtendedJobFactoryError> {
        let mut outputs: Vec<TxOut> = vec![];

        // add the outputs from the SetCustomMiningJob
        let serialized_outputs = m.coinbase_tx_outputs.inner_as_ref().to_vec();

        // The serialized outputs are in Bitcoin consensus format
        // We need to parse them one by one, keeping track of cursor position
        let mut cursor = 0;
        let mut txouts = &serialized_outputs[cursor..];

        // Iteratively decode each TxOut until we can't decode any more
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            // Calculate the size of this TxOut based on its script_pubkey length
            // 8 bytes for value + variable bytes for script_pubkey length
            // For small scripts (0-252 bytes): 1 byte length prefix
            // For medium scripts (253-1000000 bytes): 3 byte length prefix (1 marker + 2 byte
            // length)
            let len = match out.script_pubkey.len() {
                a @ 0..=252 => 8 + 1 + a,       // 8 (value) + 1 (compact size) + script_len
                a @ 253..=1000000 => 8 + 3 + a, // 8 (value) + 3 (compact size) + script_len
                _ => break,                     // Unreasonably large script, likely an error
            };

            // Move the cursor forward by the size of this TxOut
            cursor += len;
            outputs.push(out);
        }

        let mut script_sig = vec![];
        script_sig.extend_from_slice(m.coinbase_prefix.inner_as_ref());
        script_sig.extend_from_slice(&vec![
            0;
            extranonce_prefix_len
                + self.rollable_extranonce_size as usize
        ]);

        // Create transaction input
        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(m.coinbase_tx_input_n_sequence),
            witness: Witness::from(vec![] as Vec<Vec<u8>>),
        };

        Ok(Transaction {
            version: Version::non_standard(m.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(m.coinbase_tx_locktime),
            input: vec![tx_in],
            output: outputs,
        })
    }

    fn custom_coinbase_tx_prefix(
        &self,
        extranonce_prefix_len: usize,
        m: SetCustomMiningJob<'_>,
    ) -> Result<B064K<'static>, ExtendedJobFactoryError> {
        let coinbase = self.custom_coinbase(extranonce_prefix_len, m.clone())?;
        let serialized_coinbase = serialize(&coinbase);

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + m.coinbase_prefix.inner_as_ref().len(); // script_sig_prefix

        let r = serialized_coinbase[0..index].to_vec();

        r.try_into()
            .map_err(|_| ExtendedJobFactoryError::CoinbaseTxPrefixError)
    }

    fn custom_coinbase_tx_suffix(
        &self,
        extranonce_prefix_len: usize,
        m: SetCustomMiningJob<'_>,
    ) -> Result<B064K<'static>, ExtendedJobFactoryError> {
        let coinbase = self.custom_coinbase(extranonce_prefix_len, m.clone())?;
        let serialized_coinbase = serialize(&coinbase);

        // Calculate full extranonce size
        let full_extranonce_size = extranonce_prefix_len + self.rollable_extranonce_size as usize;

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + m.coinbase_prefix.inner_as_ref().len() // script_sig_prefix
            + full_extranonce_size;

        let r = serialized_coinbase[index..].to_vec();

        r.try_into()
            .map_err(|_| ExtendedJobFactoryError::CoinbaseTxSuffixError)
    }

    // build a coinbase transaction from some template in the JobFactory
    // this is only used to extract coinbase_tx_prefix and coinbase_tx_suffix
    fn coinbase(
        &self,
        extranonce_prefix_len: usize,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<Transaction, ExtendedJobFactoryError> {
        // check that the sum of the additional coinbase outputs is equal to the value remaining in
        // the active template
        let mut coinbase_reward_outputs_sum = Amount::from_sat(0);
        for output in coinbase_reward_outputs.iter() {
            coinbase_reward_outputs_sum = coinbase_reward_outputs_sum
                .checked_add(output.value)
                .ok_or(ExtendedJobFactoryError::CoinbaseOutputsSumOverflow)?;
        }

        if template.coinbase_tx_value_remaining < coinbase_reward_outputs_sum.to_sat() {
            return Err(ExtendedJobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let mut outputs = vec![];

        for output in coinbase_reward_outputs.iter() {
            outputs.push(output.clone());
        }

        let serialized_template_outputs = template.coinbase_tx_outputs.to_vec();
        let mut cursor = 0;
        let mut txouts = &serialized_template_outputs[cursor..];
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            let len = match out.script_pubkey.len() {
                a @ 0..=252 => 8 + 1 + a,
                a @ 253..=10000 => 8 + 3 + a,
                _ => break,
            };
            cursor += len;
            outputs.push(out);
        }

        let full_extranonce_size = extranonce_prefix_len + self.rollable_extranonce_size as usize;

        let mut script_sig = template.coinbase_prefix.to_vec();
        script_sig.extend_from_slice(&vec![0; full_extranonce_size]);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(template.coinbase_tx_input_sequence),
            witness: Witness::from(vec![vec![0; 32]]),
        };

        Ok(Transaction {
            version: Version::non_standard(template.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(template.coinbase_tx_locktime),
            input: vec![tx_in],
            output: outputs,
        })
    }

    fn coinbase_tx_prefix(
        &self,
        extranonce_prefix_len: usize,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<B064K<'static>, ExtendedJobFactoryError> {
        let coinbase = self.coinbase(
            extranonce_prefix_len,
            template.clone(),
            coinbase_reward_outputs,
        )?;
        let serialized_coinbase = serialize(&coinbase);

        let index = 4 // tx version
            + 2 // segwit bytes
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len(); // script_sig_prefix

        let r = serialized_coinbase[0..index].to_vec();

        r.try_into()
            .map_err(|_| ExtendedJobFactoryError::CoinbaseTxPrefixError)
    }

    fn coinbase_tx_suffix(
        &self,
        extranonce_prefix_len: usize,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<B064K<'static>, ExtendedJobFactoryError> {
        let coinbase = self.coinbase(
            extranonce_prefix_len,
            template.clone(),
            coinbase_reward_outputs,
        )?;
        let serialized_coinbase = serialize(&coinbase);

        let full_extranonce_size = extranonce_prefix_len + self.rollable_extranonce_size as usize;

        let r = serialized_coinbase[4 // tx version
            + 2 // segwit bytes
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len() // script_sig_prefix
            + full_extranonce_size..]
            .to_vec();

        r.try_into()
            .map_err(|_| ExtendedJobFactoryError::CoinbaseTxSuffixError)
    }
}

#[derive(Debug)]
pub enum ExtendedJobFactoryError {
    InvalidTemplate(String),
    CoinbaseTxPrefixError,
    CoinbaseTxSuffixError,
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
}

#[cfg(test)]
mod tests {
    use super::*;
    use stratum_common::bitcoin::ScriptBuf;
    use template_distribution_sv2::NewTemplate;

    #[test]
    fn test_new_job() {
        let mut job_factory = ExtendedJobFactory::new(5, true);

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // match the original script format used to generate the coinbase_reward_outputs for the
        // expected job
        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(5000000000),
            script_pubkey: script,
        }];

        // match the original extranonce_prefix used to generate the expected job
        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ];

        let job = job_factory
            .new_job(
                1,
                None,
                extranonce_prefix.len(),
                template,
                coinbase_reward_outputs,
            )
            .unwrap();

        // we know that the provided template should generate this job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(None),
            version: 536870912,
            version_rolling_allowed: true,
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        assert_eq!(job.job_message, expected_job);
    }
}
