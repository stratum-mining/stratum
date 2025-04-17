use crate::{
    channel_management::standard::channel::error::GroupChannelError,
    job_management::{
        chain_tip::ChainTip,
        extended::{ExtendedJob, ExtendedJobFactory},
    },
};
use mining_sv2::MAX_EXTRANONCE_LEN;
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

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
///
/// Since share validation happens at the Standard Channel level, we don't really keep track of:
/// - the group channel's past jobs
/// - the group channel's stale jobs
/// - the group channel's share validation state
#[derive(Debug, Clone)]
pub struct GroupChannel<'a> {
    group_channel_id: u32,
    standard_channel_ids: HashSet<u32>,
    job_factory: ExtendedJobFactory,
    future_jobs: HashMap<u64, ExtendedJob<'a>>,
    active_job: Option<ExtendedJob<'a>>,
}

impl<'a> GroupChannel<'a> {
    pub fn new(group_channel_id: u32) -> Self {
        Self {
            group_channel_id,
            standard_channel_ids: HashSet::new(),
            job_factory: ExtendedJobFactory::new(0, true),
            future_jobs: HashMap::new(),
            active_job: None,
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

    pub fn get_active_job(&self) -> Option<&ExtendedJob<'a>> {
        self.active_job.as_ref()
    }

    pub fn get_future_jobs(&self) -> &HashMap<u64, ExtendedJob<'a>> {
        &self.future_jobs
    }

    /// Updates the group channel state with a new template.
    ///
    /// If the template is a future template, the chain tip is not used.
    /// If the template is not a future template, the chain tip must be set.
    pub fn on_new_template(
        &mut self,
        template: NewTemplate<'a>,
        coinbase_reward_outputs: Vec<TxOut>,
        chain_tip: Option<ChainTip>,
    ) -> Result<(), GroupChannelError> {
        let new_job = self
            .job_factory
            .new_job(
                self.group_channel_id,
                chain_tip,
                MAX_EXTRANONCE_LEN,
                template.clone(),
                coinbase_reward_outputs,
            )
            .map_err(|e| GroupChannelError::JobFactoryError(e))?;

        if template.future_template {
            self.future_jobs.insert(template.template_id, new_job);
        } else {
            self.active_job = Some(new_job);
        }

        Ok(())
    }

    pub fn on_set_new_prev_hash(
        &mut self,
        set_new_prev_hash: SetNewPrevHash<'a>,
    ) -> Result<(), GroupChannelError> {
        match self.future_jobs.is_empty() {
            true => {
                // if there are no future jobs, SetNewPrevHash.template_id is ignored
                // we simply set the active job to None so that whenever
                // a NewTemplate message arrives (with future_template = false),
                // the corresponding job will be set as active
                self.active_job = None;
            }
            false => {
                // the SetNewPrevHash message was addressed to a specific future template
                if !self
                    .future_jobs
                    .contains_key(&set_new_prev_hash.template_id)
                {
                    return Err(GroupChannelError::TemplateIdNotFound);
                }

                // activate the future job
                let mut activated_job = self
                    .future_jobs
                    .remove(&set_new_prev_hash.template_id)
                    .expect("future job must exist");

                activated_job.activate(set_new_prev_hash.header_timestamp);

                self.active_job = Some(activated_job);
            }
        }

        Ok(())
    }
}
