//! Abstraction of a factory for creating Sv2 Extended or Standard Jobs.
//!
//! This module provides the [`JobFactory`] struct, which enables the creation
//! of uniquely identified Extended and Standard mining jobs as required by
//! Stratum V2 (SV2) mining servers. It manages job ID assignment, construction
//! of coinbase transactions, and correct association of template and custom job
//! parameters.
//!
//! ## Responsibilities
//!
//! - **Job ID Generation**: Ensures all jobs have unique IDs per factory instance.
//! - **Job Construction**: Builds Extended and Standard jobs from SV2 templates and custom job
//!   messages, assembling all required coinbase transaction data and metadata.
//! - **Coinbase Output Validation**: Verifies that coinbase outputs match SV2 template constraints
//!   and protocol rules.
//! - **Version Rolling**: Tracks version rolling allowance for created jobs.
//!
//! ## Usage
//!
//! Designed for mining server implementations. Use `JobFactory` to generate jobs in response to
//! incoming SV2 messages (`NewTemplate`, `SetCustomMiningJob`), ensuring protocol correctness and
//! uniqueness of job IDs.
use crate::{
    bip141::try_strip_bip141,
    chain_tip::ChainTip,
    merkle_root::merkle_root_from_path,
    outputs::deserialize_template_outputs,
    server::jobs::{error::*, extended::ExtendedJob, standard::StandardJob},
};
use binary_sv2::{Sv2Option, B0255};
use bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    consensus::{serialize, Decodable},
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
    Amount, Sequence,
};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob, MAX_EXTRANONCE_LEN};
use std::convert::TryInto;
use template_distribution_sv2::NewTemplate;

#[derive(Debug, PartialEq, Eq, Clone)]
struct JobIdFactory {
    state: u32,
}

impl JobIdFactory {
    /// Creates a new [`Id`] instance initialized to `0`.
    fn new() -> Self {
        Self { state: 0 }
    }

    /// Increments then returns the internal state on a new ID.
    fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

/// A Factory for creating Extended or Standard Jobs.
///
/// Ensures unique job ids.
///
/// Enables creation of new Extended Jobs from NewTemplate and SetCustomMiningJob messages.
///
/// Enables creation of new Standard Jobs from NewTemplate messages.
#[derive(Debug, Clone)]
pub struct JobFactory {
    job_id_factory: JobIdFactory,
    version_rolling_allowed: bool,
    pool_tag_string: Option<String>,
    miner_tag_string: Option<String>,
}

impl JobFactory {
    /// Creates a new [`JobFactory`] instance.
    ///
    /// The `pool_tag_string` and `miner_tag_string` are optional and will be added to the coinbase
    /// scriptSig.
    ///
    /// Version rolling is always allowed for standard jobs, so the `version_rolling_allowed`
    /// parameter is only relevant for creating extended jobs.
    pub fn new(
        version_rolling_allowed: bool,
        pool_tag_string: Option<String>,
        miner_tag_string: Option<String>,
    ) -> Self {
        Self {
            job_id_factory: JobIdFactory::new(),
            version_rolling_allowed,
            pool_tag_string,
            miner_tag_string,
        }
    }

    /// Returns a byte vector with the OP_PUSHBYTES opcode and the pool+miner tag.
    ///
    /// The character `/` is used as a delimiter.
    ///
    /// If no pool or miner tag is provided, the delimiters are still added.
    pub fn op_pushbytes_pool_miner_tag(&self) -> Result<Vec<u8>, JobFactoryError> {
        let mut pool_miner_tag = vec![];
        pool_miner_tag.extend_from_slice(b"/");
        if let Some(pool_tag_string) = &self.pool_tag_string {
            pool_miner_tag.extend_from_slice(pool_tag_string.as_bytes());
        }
        pool_miner_tag.extend_from_slice(b"/");
        if let Some(miner_tag_string) = &self.miner_tag_string {
            pool_miner_tag.extend_from_slice(miner_tag_string.as_bytes());
        }
        pool_miner_tag.extend_from_slice(b"/");

        // Create the proper OP_PUSHBYTES opcode based on data length
        let op_pushbytes = match pool_miner_tag.len() {
            // 100 bytes are available for scriptSig
            // subtract 5 for BIP34
            // subtract 1 for OP_PUSHBYTES opcode before pool/miner tag
            // subtract 1+32 for extranonce (OP_PUSHBYTES_32 + 32 bytes)
            len @ 1..=61 => len as u8,
            _ => return Err(JobFactoryError::CoinbaseTxPrefixError),
        };

        let mut op_pushbytes_pool_miner_tag = vec![];
        op_pushbytes_pool_miner_tag.push(op_pushbytes);
        op_pushbytes_pool_miner_tag.extend_from_slice(&pool_miner_tag);

        Ok(op_pushbytes_pool_miner_tag)
    }

    /// Creates a new job from a template.
    ///
    /// This job (and related shares) is fully committed to:
    /// - The template
    /// - The additional coinbase outputs (added to the outputs coming from the template)
    /// - The extranonce prefix of the channel at the time of job creation
    ///
    /// The optional `ChainTip` defines whether the job will be future or not.
    ///
    /// Version rolling is always allowed for standard jobs, so the `version_rolling_allowed`
    /// parameter is ignored.
    ///
    /// It's up to the caller to ensure that the sum of `additional_coinbase_outputs` is equal to
    /// available template revenue. Returns an error otherwise.
    pub fn new_standard_job<'a>(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix: Vec<u8>,
        template: NewTemplate<'a>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<StandardJob<'a>, JobFactoryError> {
        let coinbase_outputs_sum = additional_coinbase_outputs
            .iter()
            .map(|o| o.value.to_sat())
            .sum::<u64>();
        if coinbase_outputs_sum != template.coinbase_tx_value_remaining {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let job_id = self.job_id_factory.next();

        let version = template.version;

        let coinbase_tx_prefix =
            self.coinbase_tx_prefix(template.clone(), additional_coinbase_outputs.clone())?;
        let coinbase_tx_suffix =
            self.coinbase_tx_suffix(template.clone(), additional_coinbase_outputs.clone())?;
        let merkle_path = template.merkle_path.clone();
        let merkle_root = merkle_root_from_path(
            &coinbase_tx_prefix,
            &coinbase_tx_suffix,
            &extranonce_prefix,
            &merkle_path.inner_as_ref(),
        )
        .expect("merkle root must be valid")
        .try_into()
        .expect("merkle root must be 32 bytes");

        let job_message = match template.future_template {
            true => NewMiningJob {
                channel_id,
                job_id,
                min_ntime: Sv2Option::new(None),
                version,
                merkle_root,
            },
            false => {
                let min_ntime = match chain_tip {
                    Some(chain_tip) => Some(chain_tip.min_ntime()),
                    None => return Err(JobFactoryError::ChainTipRequired),
                };

                NewMiningJob {
                    channel_id,
                    job_id,
                    min_ntime: Sv2Option::new(min_ntime),
                    version,
                    merkle_root,
                }
            }
        };

        let job = StandardJob::from_template(
            template,
            extranonce_prefix,
            additional_coinbase_outputs,
            job_message,
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        Ok(job)
    }

    /// Creates a new job from a template.
    ///
    /// This job (and related shares) is fully committed to:
    /// - The template
    /// - The additional coinbase outputs (added to the outputs coming from the template)
    /// - The extranonce prefix of the channel at the time of job creation
    ///
    /// The optional `ChainTip` defines whether the job will be future or not.
    ///
    /// It's up to the caller to ensure that the sum of `additional_coinbase_outputs` is equal to
    /// available template revenue. Returns an error otherwise.
    pub fn new_extended_job<'a>(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix: Vec<u8>,
        template: NewTemplate<'a>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<ExtendedJob<'a>, JobFactoryError> {
        let coinbase_outputs_sum = additional_coinbase_outputs
            .iter()
            .map(|o| o.value.to_sat())
            .sum::<u64>();
        if coinbase_outputs_sum != template.coinbase_tx_value_remaining {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let job_id = self.job_id_factory.next();

        let version = template.version;

        let coinbase_tx_prefix =
            self.coinbase_tx_prefix(template.clone(), additional_coinbase_outputs.clone())?;
        let coinbase_tx_suffix =
            self.coinbase_tx_suffix(template.clone(), additional_coinbase_outputs.clone())?;

        // strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix
        let (coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141) =
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
                .map_err(|_| JobFactoryError::FailedToStripBip141)?
                .ok_or(JobFactoryError::FailedToStripBip141)?;

        let merkle_path = template.merkle_path.clone();

        let job_message = match template.future_template {
            true => NewExtendedMiningJob {
                channel_id,
                job_id,
                min_ntime: Sv2Option::new(None),
                version,
                version_rolling_allowed: self.version_rolling_allowed,
                merkle_path,
                coinbase_tx_prefix: coinbase_tx_prefix_stripped_bip141
                    .try_into()
                    .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)?,
                coinbase_tx_suffix: coinbase_tx_suffix_stripped_bip141
                    .try_into()
                    .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)?,
            },
            false => {
                let min_ntime = match chain_tip {
                    Some(chain_tip) => Some(chain_tip.min_ntime()),
                    None => return Err(JobFactoryError::ChainTipRequired),
                };
                NewExtendedMiningJob {
                    channel_id,
                    job_id,
                    min_ntime: Sv2Option::new(min_ntime),
                    version,
                    version_rolling_allowed: self.version_rolling_allowed,
                    merkle_path,
                    coinbase_tx_prefix: coinbase_tx_prefix_stripped_bip141
                        .try_into()
                        .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)?,
                    coinbase_tx_suffix: coinbase_tx_suffix_stripped_bip141
                        .try_into()
                        .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)?,
                }
            }
        };

        let job = ExtendedJob::from_template(
            template,
            extranonce_prefix,
            additional_coinbase_outputs,
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            job_message,
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        Ok(job)
    }

    /// Creates a new coinbase_tx_prefix and coinbase_tx_suffix from a template.
    ///
    /// To be used by a Sv2 Job Declarator Client to create a `DeclareMiningJob` message.
    ///
    /// It's up to the caller to ensure that the sum of `additional_coinbase_outputs`
    /// is equal to available template revenue. Returns an error otherwise.
    pub fn new_coinbase_tx_prefix_and_suffix(
        &self,
        template: NewTemplate<'_>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<(Vec<u8>, Vec<u8>), JobFactoryError> {
        let coinbase_outputs_sum = additional_coinbase_outputs
            .iter()
            .map(|o| o.value.to_sat())
            .sum::<u64>();
        if coinbase_outputs_sum != template.coinbase_tx_value_remaining {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let coinbase_tx_prefix =
            self.coinbase_tx_prefix(template.clone(), additional_coinbase_outputs.clone())?;
        let coinbase_tx_suffix =
            self.coinbase_tx_suffix(template.clone(), additional_coinbase_outputs.clone())?;
        Ok((coinbase_tx_prefix, coinbase_tx_suffix))
    }

    /// Creates a new `SetCustomMiningJob` message from a template.
    ///
    /// To be used by a Sv2 Job Declarator Client.
    ///
    /// It's up to the caller to ensure that the sum of the additional coinbase outputs is equal to
    /// available template revenue.
    pub fn new_custom_job<'a>(
        &self,
        channel_id: u32,
        request_id: u32,
        token: B0255<'a>,
        chain_tip: ChainTip,
        template: NewTemplate<'a>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<SetCustomMiningJob<'a>, JobFactoryError> {
        let coinbase_outputs_sum = additional_coinbase_outputs
            .iter()
            .map(|o| o.value.to_sat())
            .sum::<u64>();
        if coinbase_outputs_sum != template.coinbase_tx_value_remaining {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let template_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        let mut coinbase_tx_outputs = vec![];
        coinbase_tx_outputs.extend_from_slice(additional_coinbase_outputs.as_slice());
        coinbase_tx_outputs.extend_from_slice(template_outputs.as_slice());

        let serialized_outputs = serialize(&coinbase_tx_outputs);

        let mut coinbase_prefix = vec![];
        coinbase_prefix.extend_from_slice(&template.coinbase_prefix.to_vec());
        coinbase_prefix.extend_from_slice(&self.op_pushbytes_pool_miner_tag()?);
        coinbase_prefix.push(MAX_EXTRANONCE_LEN as u8); // OP_PUSHBYTES_32 (for the extranonce)

        let set_custom_mining_job = SetCustomMiningJob {
            channel_id,
            request_id,
            token,
            version: template.version,
            prev_hash: chain_tip.prev_hash(),
            min_ntime: chain_tip.min_ntime(),
            nbits: chain_tip.nbits(),
            coinbase_tx_version: template.coinbase_tx_version,
            coinbase_prefix: coinbase_prefix
                .try_into()
                .map_err(|_| JobFactoryError::FailedToSerializeCoinbasePrefix)?,
            coinbase_tx_input_n_sequence: template.coinbase_tx_input_sequence,
            coinbase_tx_outputs: serialized_outputs
                .try_into()
                .map_err(|_| JobFactoryError::FailedToSerializeCoinbaseOutputs)?,
            coinbase_tx_locktime: template.coinbase_tx_locktime,
            merkle_path: template.merkle_path.clone(),
        };

        Ok(set_custom_mining_job)
    }

    /// Creates a new Extended Job from a SetCustomMiningJob message.
    ///
    /// Assumes that the SetCustomMiningJob message has already been validated.
    ///
    /// To be used by Extended Channels on a Sv2 Pool Server.
    pub fn new_extended_job_from_custom_job<'a>(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'a>,
        extranonce_prefix: Vec<u8>,
    ) -> Result<ExtendedJob<'a>, JobFactoryError> {
        let serialized_outputs = set_custom_mining_job
            .coinbase_tx_outputs
            .inner_as_ref()
            .to_vec();

        let coinbase_outputs = Vec::<TxOut>::consensus_decode(&mut serialized_outputs.as_slice())
            .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        let job_id = self.job_id_factory.next();

        let version = set_custom_mining_job.version;

        let coinbase_tx_prefix = self.custom_coinbase_tx_prefix(set_custom_mining_job.clone())?;
        let coinbase_tx_suffix = self.custom_coinbase_tx_suffix(set_custom_mining_job.clone())?;

        // strip bip141 bytes from coinbase_tx_prefix and coinbase_tx_suffix
        let (coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141) =
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
                .map_err(|_| JobFactoryError::FailedToStripBip141)?
                .ok_or(JobFactoryError::FailedToStripBip141)?;

        let merkle_path = set_custom_mining_job.merkle_path.clone().into_static();

        let job_message = NewExtendedMiningJob {
            channel_id: set_custom_mining_job.channel_id,
            job_id,
            min_ntime: Sv2Option::new(Some(set_custom_mining_job.min_ntime)),
            version,
            version_rolling_allowed: self.version_rolling_allowed,
            coinbase_tx_prefix: coinbase_tx_prefix_stripped_bip141
                .clone()
                .try_into()
                .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)?,
            coinbase_tx_suffix: coinbase_tx_suffix_stripped_bip141
                .clone()
                .try_into()
                .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)?,
            merkle_path,
        };

        let job = ExtendedJob::from_custom_job(
            set_custom_mining_job,
            extranonce_prefix,
            coinbase_outputs,
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            job_message,
        );

        Ok(job)
    }
}

// impl block with private methods
impl JobFactory {
    // build a coinbase transaction from a SetCustomMiningJob
    // this is only used to extract coinbase_tx_prefix and coinbase_tx_suffix from the custom
    // coinbase
    fn custom_coinbase(&self, m: SetCustomMiningJob<'_>) -> Result<Transaction, JobFactoryError> {
        let deserialized_outputs = Vec::<TxOut>::consensus_decode(
            &mut m.coinbase_tx_outputs.inner_as_ref().to_vec().as_slice(),
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        let mut script_sig = vec![];
        script_sig.extend_from_slice(m.coinbase_prefix.inner_as_ref());
        script_sig.extend_from_slice(&[0; MAX_EXTRANONCE_LEN]);

        // Create transaction input
        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(m.coinbase_tx_input_n_sequence),
            witness: Witness::from(vec![vec![0; 32]]), /* note: 32 bytes of zeros is only safe to
                                                        * assume now, this could change in future
                                                        * soft forks */
        };

        Ok(Transaction {
            version: Version::non_standard(m.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(m.coinbase_tx_locktime),
            input: vec![tx_in],
            output: deserialized_outputs,
        })
    }

    fn custom_coinbase_tx_prefix(
        &self,
        m: SetCustomMiningJob<'_>,
    ) -> Result<Vec<u8>, JobFactoryError> {
        let coinbase = self.custom_coinbase(m.clone())?;
        let serialized_coinbase = serialize(&coinbase);

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + m.coinbase_prefix.inner_as_ref().len();

        let coinbase_tx_prefix = serialized_coinbase[0..index].to_vec();

        Ok(coinbase_tx_prefix)
    }

    fn custom_coinbase_tx_suffix(
        &self,
        m: SetCustomMiningJob<'_>,
    ) -> Result<Vec<u8>, JobFactoryError> {
        let coinbase = self.custom_coinbase(m.clone())?;
        let serialized_coinbase = serialize(&coinbase);

        // Calculate full extranonce size
        let full_extranonce_size = MAX_EXTRANONCE_LEN;

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + m.coinbase_prefix.inner_as_ref().len()
            + full_extranonce_size;

        let coinbase_tx_suffix = serialized_coinbase[index..].to_vec();

        Ok(coinbase_tx_suffix)
    }

    // build a coinbase transaction from some template in the JobFactory
    fn coinbase(
        &self,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<Transaction, JobFactoryError> {
        // check that the sum of the additional coinbase outputs is equal to the value remaining in
        // the active template
        let mut coinbase_reward_outputs_sum = Amount::from_sat(0);
        for output in coinbase_reward_outputs.iter() {
            coinbase_reward_outputs_sum = coinbase_reward_outputs_sum
                .checked_add(output.value)
                .ok_or(JobFactoryError::CoinbaseOutputsSumOverflow)?;
        }

        if template.coinbase_tx_value_remaining < coinbase_reward_outputs_sum.to_sat() {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let mut outputs = vec![];

        for output in coinbase_reward_outputs.iter() {
            outputs.push(output.clone());
        }

        let mut template_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        outputs.append(&mut template_outputs);

        let op_pushbytes_pool_miner_tag = self.op_pushbytes_pool_miner_tag()?;

        let mut script_sig = vec![];
        script_sig.extend_from_slice(&template.coinbase_prefix.to_vec());
        script_sig.extend_from_slice(&op_pushbytes_pool_miner_tag);
        script_sig.push(MAX_EXTRANONCE_LEN as u8); // OP_PUSHBYTES_32 (for the extranonce)
        script_sig.extend_from_slice(&[0; MAX_EXTRANONCE_LEN]);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(template.coinbase_tx_input_sequence),
            witness: Witness::from(vec![vec![0; 32]]), /* note: 32 bytes of zeros is only safe to
                                                        * assume now, this could change in future
                                                        * soft forks */
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
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<Vec<u8>, JobFactoryError> {
        let coinbase = self.coinbase(template.clone(), coinbase_reward_outputs)?;
        let serialized_coinbase = serialize(&coinbase);

        // Calculate the full pool/miner tag length including delimiters and OP_PUSHBYTES opcode
        let pool_miner_tag_len = 1 // OP_PUSHBYTES opcode
            + 3 // three "/" delimiters
            + self.pool_tag_string.as_ref().map_or(0, |s| s.len())
            + self.miner_tag_string.as_ref().map_or(0, |s| s.len());

        let index = 4 // tx version
            + 2 // segwit bytes
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len()
            + pool_miner_tag_len
            + 1; // OP_PUSHBYTES_32 (for the extranonce)

        let coinbase_tx_prefix = serialized_coinbase[0..index].to_vec();

        Ok(coinbase_tx_prefix)
    }

    fn coinbase_tx_suffix(
        &self,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<Vec<u8>, JobFactoryError> {
        let coinbase = self.coinbase(template.clone(), coinbase_reward_outputs)?;
        let serialized_coinbase = serialize(&coinbase);

        // Calculate the full pool/miner tag length including delimiters and OP_PUSHBYTES opcode
        let pool_miner_tag_len = 1 // OP_PUSHBYTES opcode
            + 3 // three "/" delimiters
            + self.pool_tag_string.as_ref().map_or(0, |s| s.len())
            + self.miner_tag_string.as_ref().map_or(0, |s| s.len());

        // 32 bytes
        let full_extranonce_size = MAX_EXTRANONCE_LEN;

        let coinbase_tx_suffix = serialized_coinbase[4 // tx version
            + 2 // segwit bytes
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len()
            + pool_miner_tag_len
            + 1 // OP_PUSHBYTES_32 (for the extranonce)
            + full_extranonce_size..]
            .to_vec();

        Ok(coinbase_tx_suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::ScriptBuf;
    use template_distribution_sv2::NewTemplate;

    #[test]
    fn test_new_pool_job() {
        let mut job_factory = JobFactory::new(true, Some("Stratum V2 SRI Pool".to_string()), None);

        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation

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
        ]
        .to_vec();

        let job = job_factory
            .new_extended_job(
                1,
                None,
                extranonce_prefix,
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
            // contains scriptSig with /Stratum V2 SRI Pool//
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 58, 82, 0, 22, 47, 83, 116, 114, 97,
                116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 47, 47, 32,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        assert_eq!(job.get_job_message(), &expected_job);
    }

    #[test]
    fn test_new_extended_job_from_custom_job() {
        let jdc_job_factory = JobFactory::new(
            true,
            Some("Stratum V2 SRI Pool".to_string()),
            Some("Stratum V2 SRI Miner".to_string()),
        );

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let template = NewTemplate {
            template_id: 1,
            future_template: false,
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

        let chain_tip = ChainTip::new(
            [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            503543726,
            1746839905,
        );

        let set_custom_mining_job = jdc_job_factory
            .new_custom_job(
                1,
                1,
                vec![0].try_into().unwrap(),
                chain_tip,
                template,
                coinbase_reward_outputs,
            )
            .unwrap();

        let mut pool_job_factory =
            JobFactory::new(true, Some("Stratum V2 SRI Pool".to_string()), None);

        let custom_job = pool_job_factory
            .new_extended_job_from_custom_job(set_custom_mining_job, extranonce_prefix)
            .unwrap();

        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(1746839905)),
            version: 536870912,
            version_rolling_allowed: true,
            // contains scriptSig with /Stratum V2 SRI Pool/Stratum V2 SRI Miner/
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 78, 82, 0, 42, 47, 83, 116, 114, 97,
                116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 47, 83, 116, 114,
                97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 77, 105, 110, 101, 114, 47, 32,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
            merkle_path: vec![].try_into().unwrap(),
        };

        assert_eq!(custom_job.get_job_message(), &expected_job);
    }
}
