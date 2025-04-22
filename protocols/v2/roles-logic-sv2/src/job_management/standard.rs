use binary_sv2::{Sv2Option, U256};
use mining_sv2::NewMiningJob;
use stratum_common::bitcoin::{
    absolute::LockTime,
    blockdata::witness::Witness,
    consensus::Decodable,
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
    Amount, Sequence,
};
use template_distribution_sv2::NewTemplate;

#[derive(Debug, Clone)]
pub struct StandardJob<'a> {
    template: NewTemplate<'a>,
    coinbase_reward_outputs: Vec<TxOut>,
    job_message: NewMiningJob<'a>,
}

impl<'a> StandardJob<'a> {
    pub fn new(
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
        job_message: NewMiningJob<'a>,
    ) -> Self {
        Self {
            template,
            coinbase_reward_outputs,
            job_message,
        }
    }

    pub fn get_job_id(&self) -> u32 {
        self.job_message.job_id
    }

    pub fn get_template(&self) -> &NewTemplate<'a> {
        &self.template
    }

    pub fn get_coinbase_reward_outputs(&self) -> &Vec<TxOut> {
        &self.coinbase_reward_outputs
    }

    pub fn get_job_message(&self) -> &NewMiningJob<'a> {
        &self.job_message
    }

    pub fn get_merkle_root(&self) -> &U256<'a> {
        &self.job_message.merkle_root
    }

    pub fn is_future(&self) -> bool {
        self.job_message.min_ntime.clone().into_inner().is_none()
    }

    pub fn activate(&mut self, min_ntime: u32) {
        self.job_message.min_ntime = Sv2Option::new(Some(min_ntime));
    }

    pub fn coinbase(&self, extranonce_prefix: Vec<u8>) -> Result<Transaction, StandardJobError> {
        // check that the sum of the additional coinbase outputs is equal to the value remaining in
        // the active template
        let mut coinbase_reward_outputs_sum = Amount::from_sat(0);
        for output in self.coinbase_reward_outputs.iter() {
            coinbase_reward_outputs_sum = coinbase_reward_outputs_sum
                .checked_add(output.value)
                .ok_or(StandardJobError::CoinbaseOutputsSumOverflow)?;
        }

        if self.template.coinbase_tx_value_remaining < coinbase_reward_outputs_sum.to_sat() {
            return Err(StandardJobError::InvalidCoinbaseOutputsSum);
        }

        let mut outputs = vec![];

        for output in self.coinbase_reward_outputs.iter() {
            outputs.push(output.clone());
        }

        let serialized_template_outputs = self.template.coinbase_tx_outputs.to_vec();
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

        let mut script_sig = self.template.coinbase_prefix.to_vec();
        script_sig.extend_from_slice(&extranonce_prefix);

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: script_sig.into(),
            sequence: Sequence(self.template.coinbase_tx_input_sequence),
            witness: Witness::from(vec![vec![0; 32]]),
        };

        Ok(Transaction {
            version: Version::non_standard(self.template.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(self.template.coinbase_tx_locktime),
            input: vec![tx_in],
            output: outputs,
        })
    }
}

pub enum StandardJobError {
    CoinbaseOutputsSumOverflow,
    InvalidCoinbaseOutputsSum,
}
