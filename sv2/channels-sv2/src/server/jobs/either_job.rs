use crate::{
    chain_tip::ChainTip,
    merkle_root::merkle_root_from_path,
    outputs::deserialize_template_outputs,
    server::{
        jobs::{
            error::{ExtendedJobError, StandardJobError},
            Job, JobMessage, JobOrigin,
        },
        share_accounting::{ShareValidationError, ShareValidationResult},
    },
    target::{bytes_to_hex, u256_to_block_hash},
};
use binary_sv2::{Seq0255, Sv2Option, B032, U256};
use bitcoin::{
    block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target,
};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob};
use std::convert::TryInto;
use template_distribution_sv2::NewTemplate;
use tracing::{debug, error};

#[derive(Debug, Clone, PartialEq)]
pub enum EitherJobVariant {
    Extended,
    Standard,
}

/// Abstraction of an extended mining job with:
/// - the `NewTemplate` OR `SetCustomMiningJob` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewExtendedMiningJob` message to be sent across the wire
///
/// Please note that `coinbase_tx_prefix` and `coinbase_tx_suffix` are stored in memory with bip141
/// data (marker, flag and witness). That makes it easy to reconstruct the segwit coinbase while
/// trying to propagate a block.
///
/// However, the `coinbase_tx_prefix` and `coinbase_tx_suffix` contained in the
/// `NewExtendedMiningJob` message to be sent across the wire DO NOT contain bip141 data.
/// That makes it easy to calculate the coinbase `txid` (instead of `wtxid`) for merkle root
/// calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct EitherJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    // StandardJob specific field
    merkle_root: Option<U256<'a>>,
    // ExtendedJob specific fields
    coinbase_tx_prefix_with_bip141: Vec<u8>,
    coinbase_tx_suffix_with_bip141: Vec<u8>,
    job_message: JobMessage<'a>,
    variant: EitherJobVariant,
}

impl<'a> Job<'a> for EitherJob<'a> {
    fn get_job_id(&self) -> u32 {
        self.job_message.get_job_id()
    }
    fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
    }
    fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }
    fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
    }
    fn get_job_message(&self) -> &JobMessage<'a> {
        &self.job_message
    }
    fn get_min_ntime(&self) -> Sv2Option<'a, u32> {
        self.job_message.get_min_ntime()
    }
    fn get_version(&self) -> u32 {
        self.job_message.get_version()
    }
    fn version_rolling_allowed(&self) -> bool {
        self.job_message.version_rolling_allowed()
    }
    fn get_merkle_root(&self, full_extranonce: Option<&[u8]>) -> Option<U256<'a>> {
        if let EitherJobVariant::Standard = self.variant {
            return self.merkle_root.clone();
        }
        match full_extranonce {
            Some(extranonce) => {
                let merkle_root: U256<'a> = merkle_root_from_path(
                    &&self.get_coinbase_tx_prefix_without_bip141(),
                    &self.get_coinbase_tx_suffix_without_bip141(),
                    extranonce,
                    &self.get_merkle_path().inner_as_ref(),
                )?
                .try_into()
                .ok()?;
                Some(merkle_root)
            }
            None => unreachable!(
                "full_extranonce is required to calculate merkle root for extended jobs"
            ),
        }
    }

    /// Returns the merkle path for this job.
    fn get_merkle_path(&self) -> &Seq0255<'a, U256<'a>> {
        &self.job_message.get_merkle_path()
    }
    fn get_coinbase_tx_prefix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_prefix_with_bip141.clone()
    }
    fn get_coinbase_tx_suffix_with_bip141(&self) -> Vec<u8> {
        self.coinbase_tx_suffix_with_bip141.clone()
    }
    /// Returns the coinbase transaction without for this job without BIP141 data.
    fn get_coinbase_tx_prefix_without_bip141(&self) -> Vec<u8> {
        self.job_message.get_coinbase_tx_prefix_without_bip141()
    }
    /// Returns the coinbase transaction suffix for this job without BIP141 data.
    fn get_coinbase_tx_suffix_without_bip141(&self) -> Vec<u8> {
        self.job_message.get_coinbase_tx_suffix_without_bip141()
    }
    fn is_future(&self) -> bool {
        self.job_message.get_min_ntime().0.is_empty()
    }
    fn activate(&mut self, min_ntime: u32) {
        self.job_message.activate(min_ntime);
    }
}

impl<'a> EitherJob<'a> {
    /// Creates a new standard job from a template.
    ///
    /// Combines coinbase outputs from the template and any additional outputs.
    /// Returns an error if coinbase outputs cannot be deserialized.
    pub fn standard_from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: JobMessage<'a>,
    ) -> Result<Self, StandardJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| StandardJobError::FailedToDeserializeCoinbaseOutputs)?;

        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(template_coinbase_outputs);

        Ok(Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: coinbase_tx_prefix,
            coinbase_tx_suffix_with_bip141: coinbase_tx_suffix,
            job_message,
            variant: EitherJobVariant::Standard,
        })
    }

    pub fn extended_from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Result<Self, ExtendedJobError> {
        let template_coinbase_outputs = deserialize_template_outputs(
            template.coinbase_tx_outputs.to_vec(),
            template.coinbase_tx_outputs_count,
        )
        .map_err(|_| ExtendedJobError::FailedToDeserializeCoinbaseOutputs)?;

        let mut coinbase_outputs = vec![];
        coinbase_outputs.extend(additional_coinbase_outputs);
        coinbase_outputs.extend(template_coinbase_outputs);

        Ok(Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: coinbase_tx_prefix,
            coinbase_tx_suffix_with_bip141: coinbase_tx_suffix,
            job_message: JobMessage::NewExtendedMiningJob(job_message),
            variant: EitherJobVariant::Extended,
        })
    }
    /// Creates a new extended job from a custom mining job message.
    ///
    /// Used for jobs originating from [`SetCustomMiningJob`] messages.
    pub fn from_custom_job(
        custom_job: SetCustomMiningJob<'a>,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        coinbase_tx_prefix: Vec<u8>,
        coinbase_tx_suffix: Vec<u8>,
        job_message: JobMessage<'a>,
    ) -> Self {
        Self {
            origin: JobOrigin::SetCustomMiningJob(custom_job),
            extranonce_prefix,
            coinbase_outputs,
            merkle_root: None,
            coinbase_tx_prefix_with_bip141: coinbase_tx_prefix,
            coinbase_tx_suffix_with_bip141: coinbase_tx_suffix,
            job_message,
            variant: EitherJobVariant::Extended,
        }
    }

    /// Converts the `ExtendedJob` into a `StandardJob`.
    ///
    /// Only possible if the job was created from a `NewTemplate`.
    /// Jobs created from `SetCustomMiningJob` cannot be converted
    pub fn into_standard_job(
        self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<Self, ExtendedJobError> {
        // here we can only convert extended jobs that were created from a template
        let template = match self.get_origin() {
            JobOrigin::NewTemplate(template) => template,
            JobOrigin::SetCustomMiningJob(_) => {
                return Err(ExtendedJobError::FailedToConvertToStandardJob);
            }
        };

        let merkle_root: U256<'a> = merkle_root_from_path(
            &&self.get_coinbase_tx_prefix_without_bip141(),
            &self.get_coinbase_tx_suffix_without_bip141(),
            &extranonce_prefix,
            &self.get_merkle_path().inner_as_ref(),
        )
        .ok_or(ExtendedJobError::FailedToCalculateMerkleRoot)?
        .try_into()
        .map_err(|_| ExtendedJobError::FailedToCalculateMerkleRoot)?;

        let standard_job_message = NewMiningJob {
            channel_id,
            job_id: self.get_job_id(),
            merkle_root: merkle_root.clone(),
            version: self.get_version(),
            min_ntime: self.get_min_ntime(),
        };

        let mut standard_job = EitherJob::standard_from_template(
            template.clone(),
            extranonce_prefix,
            self.get_coinbase_outputs().clone(),
            self.get_coinbase_tx_prefix_with_bip141(),
            self.get_coinbase_tx_suffix_with_bip141(),
            JobMessage::NewMiningJob(standard_job_message),
        )
        .map_err(|_| ExtendedJobError::FailedToConvertToStandardJob)?;
        standard_job.variant = EitherJobVariant::Standard;
        Ok(standard_job)
    }
}

pub struct EitherShare<'decoder> {
    pub channel_id: u32,
    pub sequence_number: u32,
    pub job_id: u32,
    pub nonce: u32,
    pub ntime: u32,
    pub version: u32,
    pub extranonce: Option<B032<'decoder>>,
}

impl<'decoder> From<mining_sv2::SubmitSharesExtended<'decoder>> for EitherShare<'decoder> {
    fn from(submit: mining_sv2::SubmitSharesExtended<'decoder>) -> Self {
        EitherShare {
            channel_id: submit.channel_id,
            sequence_number: submit.sequence_number,
            job_id: submit.job_id,
            nonce: submit.nonce,
            ntime: submit.ntime,
            version: submit.version,
            extranonce: Some(submit.extranonce),
        }
    }
}

impl<'decoder> From<mining_sv2::SubmitSharesStandard> for EitherShare<'decoder> {
    fn from(submit: mining_sv2::SubmitSharesStandard) -> Self {
        EitherShare {
            channel_id: submit.channel_id,
            sequence_number: submit.sequence_number,
            job_id: submit.job_id,
            nonce: submit.nonce,
            ntime: submit.ntime,
            version: submit.version,
            extranonce: None,
        }
    }
}

pub fn validate_either_share<'a>(
    job: impl Job<'a>,
    share: &EitherShare,
    chain_tip: &ChainTip,
    target: &Target,
    rollable_extranonce_size: Option<u16>,
) -> Result<ShareValidationResult, ShareValidationError> {
    let mut full_extranonce = vec![];
    let merkle_root: U256 = match share.extranonce {
        // extended share
        Some(ref extranonce) => {
            let extranonce_size = extranonce.inner_as_ref().len();
            if extranonce_size
                != rollable_extranonce_size
                    .ok_or(ShareValidationError::BadExtranonceSize)?
                    .into()
            {
                return Err(ShareValidationError::BadExtranonceSize);
            }

            let extranonce_prefix = job.get_extranonce_prefix();
            full_extranonce.extend(extranonce_prefix.clone());
            full_extranonce.extend(extranonce.inner_as_ref());

            // calculate the merkle root from:
            // - job coinbase_tx_prefix
            // - full extranonce
            // - job coinbase_tx_suffix
            // - job merkle_path
            let merkle_root = merkle_root_from_path(
                &job.get_coinbase_tx_prefix_without_bip141(),
                &job.get_coinbase_tx_suffix_without_bip141(),
                full_extranonce.as_ref(),
                &job.get_merkle_path().inner_as_ref(),
            )
            .ok_or(ShareValidationError::Invalid)?;
            merkle_root
                .try_into()
                .map_err(|_| ShareValidationError::Invalid)?
        }
        // standard share (merkle root is directly available)
        None => {
            if let Some(merkle_root) = job.get_merkle_root(None) {
                merkle_root
            } else {
                error!("Standard share submitted for an extended job");
                return Err(ShareValidationError::Invalid);
            }
        }
    };

    // Validate version rolling
    if !job.version_rolling_allowed() && share.version != job.get_version() {
        // If version rolling is not allowed, ensure bits 13-28 are 0
        // This is done by checking if the version & 0x1fffe000 == 0
        // ref: https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki
        if (share.version & 0x1fffe000) != 0 {
            return Err(ShareValidationError::VersionRollingNotAllowed);
        }
    }

    let prev_hash = chain_tip.prev_hash();
    let nbits = CompactTarget::from_consensus(chain_tip.nbits());

    // create the header for validation
    let header = Header {
        version: Version::from_consensus(share.version as i32),
        prev_blockhash: u256_to_block_hash(prev_hash.clone()),
        merkle_root: (*Hash::from_bytes_ref(&(merkle_root.into()))).into(),
        time: share.ntime,
        bits: nbits,
        nonce: share.nonce,
    };

    // convert the header hash to a target type for easy comparison
    let hash = header.block_hash();
    let raw_hash: [u8; 32] = *hash.to_raw_hash().as_ref();
    let block_hash_target = Target::from_le_bytes(raw_hash);
    let network_target = Target::from_compact(nbits);

    // print hash_as_target and self.target as human readable hex
    let block_hash_target_bytes = block_hash_target.to_be_bytes();
    let target_bytes = target.to_be_bytes();

    debug!(
        "share validation \nshare:\t\t{}\nchannel target:\t{}\nnetwork target:\t{}",
        bytes_to_hex(&block_hash_target_bytes),
        bytes_to_hex(&target_bytes),
        format!("{:x}", network_target)
    );

    // check if a block was found
    if network_target.is_met_by(hash) {
        let mut coinbase: Vec<u8> = vec![];
        let coinbase_tx_prefix = job.get_coinbase_tx_prefix_with_bip141();
        let coinbase_tx_suffix = job.get_coinbase_tx_suffix_with_bip141();
        coinbase.extend(coinbase_tx_prefix);
        coinbase.extend(full_extranonce.clone());
        coinbase.extend(coinbase_tx_suffix);

        match job.get_origin() {
            JobOrigin::NewTemplate(template) => {
                let template_id = template.template_id;
                return Ok(ShareValidationResult::BlockFound(
                    hash.to_raw_hash(),
                    Some(template_id),
                    coinbase,
                ));
            }
            JobOrigin::SetCustomMiningJob(_set_custom_mining_job) => {
                return Ok(ShareValidationResult::BlockFound(
                    hash.to_raw_hash(),
                    None,
                    coinbase,
                ));
            }
        }
    }

    // check if the share hash meets the channel target
    if &block_hash_target <= target {
        Ok(ShareValidationResult::Valid(hash.to_raw_hash()))
    } else {
        Err(ShareValidationError::DoesNotMeetTarget)
    }
}
