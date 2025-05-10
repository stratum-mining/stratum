use crate::{
    channels::server::jobs::{standard::StandardJob, JobOrigin},
    template_distribution_sv2::NewTemplate,
    utils::merkle_root_from_path,
};
use binary_sv2::{Seq0255, Sv2Option, B064K, U256};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob};
use std::convert::TryInto;
use stratum_common::bitcoin::{consensus::Decodable, transaction::TxOut};

/// Abstraction of an extended mining job with:
/// - the `NewTemplate` OR `SetCustomMiningJob` message that originated it
/// - the extranonce prefix associated with the channel at the time of job creation
/// - all coinbase outputs (spendable + unspendable) associated with the job
/// - the `NewExtendedMiningJob` message to be sent across the wire
#[derive(Debug, Clone)]
pub struct ExtendedJob<'a> {
    origin: JobOrigin<'a>,
    extranonce_prefix: Vec<u8>,
    coinbase_outputs: Vec<TxOut>,
    job_message: NewExtendedMiningJob<'a>,
}

impl<'a> ExtendedJob<'a> {
    /// Creates a new job from a template.
    ///
    /// `additional_coinbase_outputs` are the outputs after the outputs coming from the template.
    pub fn from_template(
        template: NewTemplate<'a>,
        extranonce_prefix: Vec<u8>,
        additional_coinbase_outputs: Vec<TxOut>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Self {
        // Parse the serialized outputs into a Vec<TxOut>
        let mut coinbase_outputs: Vec<TxOut> = vec![];

        // The serialized outputs coming from the template are in Bitcoin consensus format
        // We need to parse them one by one, keeping track of cursor position
        let serialized_outputs = template.coinbase_tx_outputs.inner_as_ref().to_vec();
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
            coinbase_outputs.push(out);
        }

        coinbase_outputs.extend(additional_coinbase_outputs);

        // Iteratively decode each TxOut until we can't decode any more
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            coinbase_outputs.push(out);
        }

        Self {
            origin: JobOrigin::NewTemplate(template),
            extranonce_prefix,
            coinbase_outputs,
            job_message,
        }
    }

    pub fn from_custom_job(
        custom_job: SetCustomMiningJob<'a>,
        extranonce_prefix: Vec<u8>,
        coinbase_outputs: Vec<TxOut>,
        job_message: NewExtendedMiningJob<'a>,
    ) -> Self {
        Self {
            origin: JobOrigin::SetCustomMiningJob(custom_job),
            extranonce_prefix,
            coinbase_outputs,
            job_message,
        }
    }

    /// Converts the `ExtendedJob` into a `SetCustomMiningJob` message.
    ///
    /// To be used by a Sv2 Job Declaration Client after:
    /// - creating a non-future extended job from a non-future template
    /// - activating a future extended job (that was created from a future template)
    pub fn into_custom_job(self) -> SetCustomMiningJob<'a> {
        todo!()
    }

    pub fn into_standard_job(self, channel_id: u32, extranonce_prefix: Vec<u8>) -> StandardJob<'a> {
        let merkle_root = merkle_root_from_path(
            self.get_coinbase_tx_prefix().inner_as_ref(),
            self.get_coinbase_tx_suffix().inner_as_ref(),
            &extranonce_prefix,
            &self.get_merkle_path().inner_as_ref(),
        )
        .expect("merkle root must be valid")
        .try_into()
        .expect("merkle root must be 32 bytes");

        let standard_job_message = NewMiningJob {
            channel_id,
            job_id: self.get_job_id(),
            merkle_root,
            version: self.get_version(),
            min_ntime: self.get_min_ntime(),
        };

        let standard_job = StandardJob::new(
            self.get_origin().clone(),
            extranonce_prefix,
            self.get_coinbase_outputs().clone(),
            standard_job_message,
        );

        standard_job
    }

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_origin(&self) -> &JobOrigin<'a> {
        &self.origin
    }

    pub fn get_coinbase_tx_prefix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_prefix
    }

    pub fn get_coinbase_tx_suffix(&self) -> &B064K<'a> {
        &self.job_message.coinbase_tx_suffix
    }

    pub fn get_extranonce_prefix(&self) -> &Vec<u8> {
        &self.extranonce_prefix
    }

    pub fn get_coinbase_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_outputs
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
