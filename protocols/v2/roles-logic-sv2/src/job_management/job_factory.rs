use crate::{
    channel_management::chain_tip::ChainTip,
    job_management::{ExtendedJob, StandardJob},
    template_distribution_sv2::NewTemplate,
    utils::{merkle_root_from_path, Id},
};
use binary_sv2::{Sv2Option, B064K};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob};
use std::convert::TryInto;
use stratum_common::bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    consensus::{serialize, Decodable},
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
    Amount, Sequence,
};
/// A factory for creating jobs (either standard or extended).
///
/// Factory state consists of:
/// - 1 Future + 1 Active Templates
/// - Multiple Coinbase Outputs
/// - Job ID Factory
/// - Rollable Extranonce Size
/// - Version Rolling Allowed
#[derive(Clone, Debug)]
pub struct JobFactory {
    future_template: Option<NewTemplate<'static>>,
    active_template: Option<NewTemplate<'static>>,
    additional_coinbase_outputs: Vec<TxOut>,
    job_id_factory: Id,
    rollable_extranonce_size: Option<u8>, // None if standard job factory
    version_rolling_allowed: Option<bool>, // None if standard job factory
}

// impl block with public methods
impl JobFactory {
    /// Create a new job factory.
    ///
    /// If the intended job factory is for standard jobs, set extranonce_len and
    /// version_rolling_allowed to None.
    pub fn new(
        rollable_extranonce_size: Option<u8>,
        version_rolling_allowed: Option<bool>,
    ) -> Self {
        Self {
            future_template: None,
            active_template: None,
            additional_coinbase_outputs: vec![],
            job_id_factory: Id::new(),
            rollable_extranonce_size,
            version_rolling_allowed,
        }
    }

    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.future_template = Some(template);
    }

    pub fn set_active_template(&mut self, template: NewTemplate<'static>) {
        self.active_template = Some(template);
    }

    pub fn set_additional_coinbase_outputs(&mut self, coinbase_outputs: Vec<TxOut>) {
        self.additional_coinbase_outputs = coinbase_outputs;
    }

    /// Create a new standard job.
    ///
    /// If the chain tip is provided, the job will be created from the active template.
    /// Otherwise, the job will be created from the future template.
    pub fn new_standard_job(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix: Vec<u8>,
    ) -> Result<StandardJob<'static>, JobFactoryError> {
        let job_id = self.job_id_factory.next();

        let (template, ntime) = match chain_tip {
            Some(chain_tip) => (
                self.active_template
                    .as_ref()
                    .ok_or(JobFactoryError::NoActiveTemplate)?,
                Some(chain_tip.min_ntime()),
            ),
            None => (
                self.future_template
                    .as_ref()
                    .ok_or(JobFactoryError::NoFutureTemplate)?,
                None,
            ),
        };

        let version = template.version;
        let merkle_path = template.merkle_path.inner_as_ref().to_vec();
        let coinbase_tx_prefix = self.coinbase_tx_prefix(false)?.to_vec();
        let coinbase_tx_suffix = self
            .coinbase_tx_suffix(false, extranonce_prefix.len())?
            .to_vec();
        let merkle_root = merkle_root_from_path(
            &coinbase_tx_prefix,
            &coinbase_tx_suffix,
            &extranonce_prefix,
            &merkle_path,
        )
        .ok_or(JobFactoryError::MerkleRootError)?;
        let mut coinbase = vec![];
        coinbase.extend(coinbase_tx_prefix);
        coinbase.extend(extranonce_prefix);
        coinbase.extend(coinbase_tx_suffix);

        let job_message = NewMiningJob {
            channel_id,
            job_id,
            min_ntime: Sv2Option::new(ntime),
            version,
            merkle_root: merkle_root
                .try_into()
                .map_err(|_| JobFactoryError::MerkleRootError)?,
        };

        let job = StandardJob::new(template.template_id, job_message, coinbase);

        Ok(job)
    }

    /// Create a new extended job.
    ///
    /// If the chain tip is provided, the job will be created from the future template.
    /// Otherwise, the job will be created from the active template.
    pub fn new_extended_job(
        &mut self,
        channel_id: u32,
        chain_tip: Option<ChainTip>,
        extranonce_prefix_len: usize,
    ) -> Result<ExtendedJob<'static>, JobFactoryError> {
        let version_rolling_allowed = match self.version_rolling_allowed {
            Some(version_rolling_allowed) => version_rolling_allowed,
            None => return Err(JobFactoryError::JobFactoryDoesNotSupportExtendedJobs),
        };

        let job_id = self.job_id_factory.next();

        let (template, ntime) = match chain_tip {
            Some(chain_tip) => (
                self.active_template
                    .as_ref()
                    .ok_or(JobFactoryError::NoActiveTemplate)?,
                Some(chain_tip.min_ntime()),
            ),
            None => (
                self.future_template
                    .as_ref()
                    .ok_or(JobFactoryError::NoFutureTemplate)?,
                None,
            ),
        };

        let version = template.version;
        let coinbase_tx_prefix = self.coinbase_tx_prefix(false)?;
        let coinbase_tx_suffix = self.coinbase_tx_suffix(false, extranonce_prefix_len)?;
        let merkle_path = template.merkle_path.clone();

        let job_message = NewExtendedMiningJob {
            channel_id,
            job_id,
            min_ntime: Sv2Option::new(ntime),
            version,
            version_rolling_allowed,
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            merkle_path,
        };

        let job = ExtendedJob::new(Some(template.template_id), job_message);

        Ok(job)
    }

    pub fn new_custom_job<'a>(
        &mut self,
        m: SetCustomMiningJob<'a>,
    ) -> Result<ExtendedJob<'static>, JobFactoryError> {
        let version_rolling_allowed = match self.version_rolling_allowed {
            Some(version_rolling_allowed) => version_rolling_allowed,
            None => return Err(JobFactoryError::JobFactoryDoesNotSupportExtendedJobs),
        };

        let job_id = self.job_id_factory.next();

        let version = m.version;
        let coinbase_tx_prefix = self.custom_coinbase_tx_prefix(m.clone())?;
        let coinbase_tx_suffix = self.custom_coinbase_tx_suffix(m.clone())?;
        let merkle_path = m.merkle_path.clone().into_static();

        let job_message = NewExtendedMiningJob {
            channel_id: m.channel_id,
            job_id,
            min_ntime: Sv2Option::new(Some(m.min_ntime)).into_static(),
            version,
            version_rolling_allowed,
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            merkle_path,
        };

        let job = ExtendedJob::new(None, job_message);

        Ok(job)
    }
}

// impl block with private methods
impl JobFactory {
    // build a coinbase transaction from a SetCustomMiningJob
    // this is only used to extract coinbase_tx_prefix and coinbase_tx_suffix from the custom
    // coinbase
    fn custom_coinbase<'a>(
        &self,
        m: SetCustomMiningJob<'a>,
    ) -> Result<Transaction, JobFactoryError> {
        let mut outputs: Vec<TxOut> = vec![];

        // add the outputs from the SetCustomMiningJob
        let serialized_outputs = m.coinbase_tx_outputs.inner_as_ref().to_vec();
        let mut cursor = 0;
        let mut txouts = &serialized_outputs[cursor..];
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            let len = match out.script_pubkey.len() {
                a @ 0..=252 => 8 + 1 + a,
                a @ 253..=10000 => 8 + 3 + a,
                _ => break,
            };
            cursor += len;
            outputs.push(out);
        }

        // Create transaction input
        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: m.coinbase_prefix.inner_as_ref().to_vec().into(),
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

    fn custom_coinbase_tx_prefix<'a>(
        &self,
        m: SetCustomMiningJob<'a>,
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

        Ok(r.try_into()
            .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)?)
    }

    fn custom_coinbase_tx_suffix<'a>(
        &self,
        m: SetCustomMiningJob<'a>,
    ) -> Result<B064K<'static>, JobFactoryError> {
        let coinbase = self.custom_coinbase(m.clone())?;
        let serialized_coinbase = serialize(&coinbase);

        // Calculate full extranonce size
        let extranonce_size = match self.rollable_extranonce_size {
            Some(rollable_extranonce_size) => {
                m.extranonce_size as usize + rollable_extranonce_size as usize
            }
            None => m.extranonce_size as usize,
        };

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + m.coinbase_prefix.inner_as_ref().len() // script_sig_prefix
            + extranonce_size;

        let r = serialized_coinbase[index..].to_vec();

        Ok(r.try_into()
            .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)?)
    }

    // build a coinbase transaction from some template in the JobFactory
    // this is only used to extract coinbase_tx_prefix and coinbase_tx_suffix
    fn coinbase(&self, future: bool) -> Result<Transaction, JobFactoryError> {
        let template = match future {
            true => self
                .future_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
            false => self
                .active_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
        };

        // check that the sum of the additional coinbase outputs is equal to the value remaining in
        // the active template
        let mut additional_coinbase_outputs_sum = Amount::from_sat(0);
        for output in self.additional_coinbase_outputs.iter() {
            additional_coinbase_outputs_sum = additional_coinbase_outputs_sum
                .checked_add(output.value)
                .ok_or(JobFactoryError::CoinbaseOutputsSumOverflow)?;
        }

        if template.coinbase_tx_value_remaining != additional_coinbase_outputs_sum.to_sat() {
            return Err(JobFactoryError::InvalidCoinbaseOutputsSum);
        }

        let mut outputs = vec![];

        for output in self.additional_coinbase_outputs.iter() {
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

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: template.coinbase_prefix.to_vec().into(),
            sequence: Sequence(template.coinbase_tx_input_sequence),
            witness: Witness::from(vec![] as Vec<Vec<u8>>),
        };

        Ok(Transaction {
            version: Version::non_standard(template.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(template.coinbase_tx_locktime),
            input: vec![tx_in],
            output: outputs,
        })
    }

    fn coinbase_tx_prefix(&self, future: bool) -> Result<B064K<'static>, JobFactoryError> {
        let template = match future {
            true => self
                .future_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
            false => self
                .active_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
        };

        let coinbase = self.coinbase(future)?;

        let serialized_coinbase = serialize(&coinbase);

        let index = 4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len(); // script_sig_prefix

        let r = serialized_coinbase[0..index].to_vec();

        Ok(r.try_into()
            .map_err(|_| JobFactoryError::CoinbaseTxPrefixError)?)
    }

    fn coinbase_tx_suffix(
        &self,
        future: bool,
        extranonce_prefix_len: usize,
    ) -> Result<B064K<'static>, JobFactoryError> {
        let template = match future {
            true => self
                .future_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
            false => self
                .active_template
                .as_ref()
                .ok_or(JobFactoryError::NoActiveTemplate)?,
        };

        let coinbase = self.coinbase(future)?;

        let serialized_coinbase = serialize(&coinbase);

        let full_extranonce_size = match self.rollable_extranonce_size {
            Some(rollable_extranonce_size) => {
                extranonce_prefix_len + rollable_extranonce_size as usize
            }
            None => extranonce_prefix_len,
        };

        let r = serialized_coinbase[4 // tx version
            + 2 // segwit
            + 1 // number of inputs
            + 32 // prev OutPoint
            + 4 // index
            + 1 // bytes in script
            + template.coinbase_prefix.len() // script_sig_prefix
            + full_extranonce_size..]
            .to_vec();

        Ok(r.try_into()
            .map_err(|_| JobFactoryError::CoinbaseTxSuffixError)?)
    }
}

pub enum JobFactoryError {
    NoActiveTemplate,
    NoFutureTemplate,
    MerkleRootError,
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
    TxVersionTooBig,
    CoinbaseTxPrefixError,
    CoinbaseTxSuffixError,
    JobFactoryDoesNotSupportExtendedJobs,
}
