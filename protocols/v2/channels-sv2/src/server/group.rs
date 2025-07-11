//! Abstraction over the state of a Sv2 Group Channel, as seen by a Mining Server
use crate::{
    chain_tip::ChainTip,
    server::{
        error::GroupChannelError,
        jobs::{extended::ExtendedJob, factory::JobFactory, job_store::JobStore},
    },
};
use bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp};

use std::collections::{HashMap, HashSet};

/// Abstraction of a Group Channel.
///
/// It keeps track of:
/// - the group channel's unique `group_channel_id`
/// - the group channel's `standard_channels` (indexed by `channel_id`)
/// - the group channel's job factory
/// - the group channel's future jobs (indexed by `template_id`, to be activated upon receipt of a
///   `SetNewPrevHash` message)
/// - the group channel's active job
/// - the group channel's chain tip
///
/// Since share validation happens at the Standard Channel level, we don't really keep track of:
/// - the group channel's past jobs
/// - the group channel's stale jobs
/// - the group channel's share validation state
#[derive(Debug)]
pub struct GroupChannel<'a> {
    group_channel_id: u32,
    standard_channel_ids: HashSet<u32>,
    job_factory: JobFactory,
    job_store: Box<dyn JobStore<ExtendedJob<'a>>>,
    chain_tip: Option<ChainTip>,
}

impl<'a> GroupChannel<'a> {
    pub fn new(group_channel_id: u32, job_store: Box<dyn JobStore<ExtendedJob<'a>>>) -> Self {
        Self {
            group_channel_id,
            standard_channel_ids: HashSet::new(),
            job_factory: JobFactory::new(true),
            job_store,
            chain_tip: None,
        }
    }

    pub fn add_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.insert(standard_channel_id);
    }

    pub fn remove_standard_channel_id(&mut self, standard_channel_id: u32) {
        self.standard_channel_ids.remove(&standard_channel_id);
    }

    pub fn get_group_channel_id(&self) -> u32 {
        self.group_channel_id
    }

    pub fn get_standard_channel_ids(&self) -> &HashSet<u32> {
        &self.standard_channel_ids
    }

    pub fn get_chain_tip(&self) -> Option<&ChainTip> {
        self.chain_tip.as_ref()
    }

    /// Only for testing purposes, not meant to be used in real apps.
    #[cfg(test)]
    pub fn set_chain_tip(&mut self, chain_tip: ChainTip) {
        self.chain_tip = Some(chain_tip);
    }

    pub fn get_active_job(&self) -> Option<&ExtendedJob<'a>> {
        self.job_store.get_active_job()
    }

    pub fn get_future_template_to_job_id(&self) -> &HashMap<u64, u32> {
        self.job_store.get_future_template_to_job_id()
    }

    pub fn get_future_jobs(&self) -> &HashMap<u32, ExtendedJob<'a>> {
        self.job_store.get_future_jobs()
    }

    /// Updates the group channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    pub fn on_new_template(
        &mut self,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), GroupChannelError> {
        match template.future_template {
            true => {
                let new_job = self
                    .job_factory
                    .new_extended_job(
                        self.group_channel_id,
                        None,
                        vec![], /* empty extranonce prefix, as it will be replaced by the
                                 * standard channel's extranonce
                                 * prefix */
                        template.clone(),
                        coinbase_reward_outputs,
                    )
                    .map_err(GroupChannelError::JobFactoryError)?;
                self.job_store.add_future_job(template.template_id, new_job);
            }
            false => {
                match self.chain_tip.clone() {
                    // we can only create non-future jobs if we have a chain tip
                    None => return Err(GroupChannelError::ChainTipNotSet),
                    Some(chain_tip) => {
                        let new_job = self
                            .job_factory
                            .new_extended_job(
                                self.group_channel_id,
                                Some(chain_tip),
                                vec![], /* empty extranonce prefix, as it will be replaced by
                                         * the standard
                                         * channel's extranonce prefix */
                                template.clone(),
                                coinbase_reward_outputs,
                            )
                            .map_err(GroupChannelError::JobFactoryError)?;
                        self.job_store.set_active_job(new_job);
                    }
                }
            }
        }
        Ok(())
    }

    /// Updates the channel state with a new `SetNewPrevHash` message (Template Distribution
    /// Protocol variant).
    ///
    /// If there is some future job matching the `template_id`` that `SetNewPrevHash` points to,
    /// this future job is "activated" and set as the active job.
    ///
    /// The chain tip information is not kept in the channel state.
    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHashTdp<'a>,
    ) -> Result<(), GroupChannelError> {
        match self.job_store.get_future_jobs().is_empty() {
            true => {
                return Err(GroupChannelError::TemplateIdNotFound);
            }
            false => {
                self.job_store.activate_future_job(
                    set_new_prev_hash.template_id,
                    set_new_prev_hash.header_timestamp,
                );
            }
        }

        // update the chain tip
        let set_new_prev_hash_static = set_new_prev_hash.into_static();
        let new_chain_tip = ChainTip::new(
            set_new_prev_hash_static.prev_hash,
            set_new_prev_hash_static.n_bits,
            set_new_prev_hash_static.header_timestamp,
        );
        self.chain_tip = Some(new_chain_tip);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        chain_tip::ChainTip,
        server::{group::GroupChannel, jobs::job_store::DefaultJobStore},
    };
    use binary_sv2::Sv2Option;
    use bitcoin::{transaction::TxOut, Amount, ScriptBuf};
    use mining_sv2::NewExtendedMiningJob;
    use std::convert::TryInto;
    use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

    const SATS_AVAILABLE_IN_TEMPLATE: u64 = 5000000000;

    #[test]
    fn test_future_job_activation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let group_channel_id = 1;
        let job_store = Box::new(DefaultJobStore::new());
        let mut group_channel = GroupChannel::new(group_channel_id, job_store);

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
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
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        assert!(group_channel.get_future_jobs().is_empty());
        group_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();
        assert!(group_channel.get_active_job().is_none());

        let future_job_id = group_channel
            .get_future_template_to_job_id()
            .get(&template.template_id)
            .unwrap();

        let future_job = group_channel
            .get_future_jobs()
            .get(future_job_id)
            .unwrap()
            .clone();

        // we know that the provided template + coinbase_reward_outputs should generate this future
        // job
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

        assert_eq!(future_job.get_job_message(), &expected_job);

        let ntime = 1746839905;

        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        group_channel
            .on_set_new_prev_hash(set_new_prev_hash)
            .unwrap();

        // we just activated the only future job
        assert!(group_channel.get_active_job().is_some());

        let mut previously_future_job = future_job.clone();
        previously_future_job.activate(ntime);

        let activated_job = group_channel.get_active_job().unwrap();

        // assert that the activated job is the same as the previously future job
        assert_eq!(
            activated_job.get_job_message(),
            previously_future_job.get_job_message()
        );
    }

    #[test]
    fn test_non_future_job_creation_flow() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let group_channel_id = 1;

        let job_store = Box::new(DefaultJobStore::new());
        let mut group_channel = GroupChannel::new(group_channel_id, job_store);

        let ntime = 1746839905;
        let prev_hash = [
            200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144, 205,
            88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
        ]
        .into();
        let n_bits = 503543726;

        let chain_tip = ChainTip::new(prev_hash, n_bits, ntime);
        let template = NewTemplate {
            template_id: 1,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
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
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: script,
        }];

        group_channel.set_chain_tip(chain_tip);
        group_channel
            .on_new_template(template.clone(), coinbase_reward_outputs)
            .unwrap();

        let active_job = group_channel.get_active_job().unwrap();

        // we know that the provided template + coinbase_reward_outputs should generate this
        // non-future job
        let expected_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 1,
            min_ntime: Sv2Option::new(Some(ntime)),
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

        assert_eq!(active_job.get_job_message(), &expected_job);
    }

    #[test]
    fn test_coinbase_reward_outputs_sum_above_template_value() {
        // note:
        // the messages on this test were collected from a sane message flow
        // we use them as test vectors to assert correct behavior of job creation
        let group_channel_id = 1;

        let job_store = Box::new(DefaultJobStore::new());
        let mut group_channel = GroupChannel::new(group_channel_id, job_store);

        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
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

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);

        let invalid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE + 1), /* oops: one too many extra
                                                                      * sats */
            script_pubkey: script,
        }];

        assert!(group_channel
            .on_new_template(template.clone(), invalid_coinbase_reward_outputs)
            .is_err());

        assert!(group_channel.get_future_jobs().is_empty());
    }
}
