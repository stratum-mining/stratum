//! Abstraction of a factory for creating Sv2 Extended or Standard Jobs.
use crate::{
    channels::{
        chain_tip::ChainTip,
        server::jobs::{error::*, extended::ExtendedJob, standard::StandardJob},
    },
    template_distribution_sv2::NewTemplate,
    utils::{deserialize_template_outputs, merkle_root_from_path, Id as JobIdFactory},
};
use bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    consensus::{serialize, Decodable},
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
    Amount, Sequence,
};
use codec_sv2::binary_sv2::{Sv2Option, B064K};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob, MAX_EXTRANONCE_LEN};
use std::convert::TryInto;

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
}

impl JobFactory {
    pub fn new(version_rolling_allowed: bool) -> Self {
        Self {
            job_id_factory: JobIdFactory::new(),
            version_rolling_allowed,
        }
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
    /// Note: version rolling is always allowed for standard jobs, so the `version_rolling_allowed`
    /// parameter is ignored.
    pub fn new_standard_job<'a>(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix: Vec<u8>,
        template: NewTemplate<'a>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<StandardJob<'a>, JobFactoryError> {
        let job_id = self.job_id_factory.next();

        let version = template.version;

        let coinbase_tx_prefix =
            self.coinbase_tx_prefix(template.clone(), additional_coinbase_outputs.clone())?;
        let coinbase_tx_suffix =
            self.coinbase_tx_suffix(template.clone(), additional_coinbase_outputs.clone())?;
        let merkle_path = template.merkle_path.clone();
        let merkle_root = merkle_root_from_path(
            coinbase_tx_prefix.inner_as_ref(),
            coinbase_tx_suffix.inner_as_ref(),
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
    pub fn new_extended_job<'a>(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix: Vec<u8>,
        template: NewTemplate<'a>,
        additional_coinbase_outputs: Vec<TxOut>,
    ) -> Result<ExtendedJob<'a>, JobFactoryError> {
        let job_id = self.job_id_factory.next();

        let version = template.version;

        let coinbase_tx_prefix =
            self.coinbase_tx_prefix(template.clone(), additional_coinbase_outputs.clone())?;
        let coinbase_tx_suffix =
            self.coinbase_tx_suffix(template.clone(), additional_coinbase_outputs.clone())?;
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
                    coinbase_tx_prefix,
                    coinbase_tx_suffix,
                }
            }
        };

        let job = ExtendedJob::from_template(
            template,
            extranonce_prefix,
            additional_coinbase_outputs,
            job_message,
        )
        .map_err(|_| JobFactoryError::DeserializeCoinbaseOutputsError)?;

        Ok(job)
    }

    /// Creates a new job from a SetCustomMiningJob message.
    ///
    /// Assumes that the SetCustomMiningJob message has already been validated.
    pub fn new_custom_job<'a>(
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

        let job = ExtendedJob::from_custom_job(
            set_custom_mining_job,
            extranonce_prefix,
            coinbase_outputs,
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
            witness: Witness::from(vec![vec![0; 32]]),
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
    ) -> Result<B064K<'static>, JobFactoryError> {
        let coinbase = self.custom_coinbase(m.clone())?;
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
            .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)
    }

    fn custom_coinbase_tx_suffix(
        &self,
        m: SetCustomMiningJob<'_>,
    ) -> Result<B064K<'static>, JobFactoryError> {
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
            + m.coinbase_prefix.inner_as_ref().len() // script_sig_prefix
            + full_extranonce_size;

        let r = serialized_coinbase[index..].to_vec();

        r.try_into()
            .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)
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

        let mut script_sig = vec![];
        script_sig.extend_from_slice(&template.coinbase_prefix.to_vec());
        script_sig.extend_from_slice(&[0; MAX_EXTRANONCE_LEN]);

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
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<B064K<'static>, JobFactoryError> {
        let coinbase = self.coinbase(template.clone(), coinbase_reward_outputs)?;
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
            .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)
    }

    fn coinbase_tx_suffix(
        &self,
        template: NewTemplate<'_>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<B064K<'static>, JobFactoryError> {
        let coinbase = self.coinbase(template.clone(), coinbase_reward_outputs)?;
        let serialized_coinbase = serialize(&coinbase);

        let full_extranonce_size = MAX_EXTRANONCE_LEN;

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
            .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::ScriptBuf;
    use template_distribution_sv2::NewTemplate;

    #[test]
    fn test_new_job() {
        let mut job_factory = JobFactory::new(true);

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

        assert_eq!(job.get_job_message(), &expected_job);
    }

    #[test]
    fn test_new_custom_job() {
        let mut job_factory = JobFactory::new(true);

        let extranonce_prefix = [
            83, 116, 114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 0,
            0, 0, 0, 0, 0, 0, 1,
        ]
        .to_vec();

        let set_custom_mining_job = SetCustomMiningJob {
            channel_id: 1,
            request_id: 0,
            token: vec![0].try_into().unwrap(),
            version: 536870912,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            min_ntime: 1746839905,
            nbits: 503543726,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_n_sequence: 4294967295,
            coinbase_tx_outputs: vec![
                2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220, 194, 147, 204, 170,
                14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0, 0, 0, 0, 0, 0, 38,
                106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222, 253, 63, 169, 153,
                223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139, 235, 216, 54, 151,
                78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(1746839905)),
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

        let job = job_factory
            .new_custom_job(set_custom_mining_job, extranonce_prefix)
            .unwrap();

        assert_eq!(job.get_job_message(), &expected_job);
    }
}
