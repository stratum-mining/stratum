use crate::{common_properties::StandardChannel, parsers::Mining, Error};

use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, OpenStandardMiningChannelSuccess, SetNewPrevHash,
};

use super::extended_to_standard_job;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct GroupChannels {
    channels: HashMap<u32, GroupChannel>,
}
impl GroupChannels {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }
    pub fn on_channel_success_for_hom_downtream(
        &mut self,
        m: &OpenStandardMiningChannelSuccess,
    ) -> Result<Vec<Mining<'static>>, Error> {
        let group_id = m.group_channel_id;

        self.channels
            .entry(group_id)
            .or_insert_with(GroupChannel::new);
        match self.channels.get_mut(&group_id) {
            Some(group) => group.on_channel_success_for_hom_downtream(m.clone()),
            None => unreachable!(),
        }
    }
    pub fn update_new_prev_hash(&mut self, m: &SetNewPrevHash) {
        for group in self.channels.values_mut() {
            group.update_new_prev_hash(m);
        }
    }
    pub fn on_new_extended_mining_job(&mut self, m: &NewExtendedMiningJob) {
        for group in &mut self.channels.values_mut() {
            let cloned = NewExtendedMiningJob {
                channel_id: m.channel_id,
                job_id: m.job_id,
                future_job: m.future_job,
                version: m.version,
                version_rolling_allowed: m.version_rolling_allowed,
                merkle_path: m.merkle_path.clone().into_static(),
                coinbase_tx_prefix: m.coinbase_tx_prefix.clone().into_static(),
                coinbase_tx_suffix: m.coinbase_tx_suffix.clone().into_static(),
            };
            group.on_new_extended_mining_job(cloned);
        }
    }
    pub fn last_received_job_to_standard_job(
        &mut self,
        channel_id: u32,
        group_id: u32,
    ) -> Result<NewMiningJob<'static>, Error> {
        match self.channels.get_mut(&group_id) {
            Some(group) => group.last_received_job_to_standard_job(channel_id),
            None => Err(Error::GroupIdNotFound),
        }
    }
}

#[derive(Debug, Clone)]
struct GroupChannel {
    hom_downstreams: HashMap<u32, StandardChannel>,
    future_jobs: Vec<NewExtendedMiningJob<'static>>,
    last_prev_hash: Option<SetNewPrevHash<'static>>,
    last_valid_job: Option<NewExtendedMiningJob<'static>>,
    last_received_job: Option<NewExtendedMiningJob<'static>>,
}

impl GroupChannel {
    fn new() -> Self {
        Self {
            hom_downstreams: HashMap::new(),
            future_jobs: vec![],
            last_prev_hash: None,
            last_valid_job: None,
            last_received_job: None,
        }
    }
    fn on_channel_success_for_hom_downtream(
        &mut self,
        m: OpenStandardMiningChannelSuccess,
    ) -> Result<Vec<Mining<'static>>, Error> {
        let channel = StandardChannel {
            channel_id: m.channel_id,
            group_id: m.group_channel_id,
            target: m.target.clone().into(),
            extranonce: m.extranonce_prefix.clone().into(),
        };
        let channel_id = m.channel_id;
        let mut res = vec![];
        //res.push(Mining::OpenStandardMiningChannelSuccess(m));
        for extended_job in &self.future_jobs {
            let standard_job = extended_to_standard_job(
                extended_job,
                &channel.extranonce.clone().to_vec(),
                channel.channel_id,
                None,
            )
            .ok_or(Error::ImpossibleToCalculateMerkleRoot)?;
            res.push(Mining::NewMiningJob(standard_job));
        }

        if let Some(new_prev_hash) = &self.last_prev_hash {
            res.push(Mining::SetNewPrevHash(new_prev_hash.clone()))
        };

        if let Some(last_valid_job) = &self.last_valid_job {
            let standard_job = extended_to_standard_job(
                last_valid_job,
                &channel.extranonce.clone().to_vec(),
                channel.channel_id,
                None,
            )
            .ok_or(Error::ImpossibleToCalculateMerkleRoot)?;
            res.push(Mining::NewMiningJob(standard_job));
        };

        self.hom_downstreams.insert(channel_id, channel);

        Ok(res)
    }
    fn update_new_prev_hash(&mut self, m: &SetNewPrevHash) {
        while let Some(job) = self.future_jobs.pop() {
            if job.job_id == m.job_id {
                self.last_valid_job = Some(job);
            }
        }
        self.future_jobs = vec![];
        let cloned = SetNewPrevHash {
            channel_id: m.channel_id,
            job_id: m.job_id,
            prev_hash: m.prev_hash.clone().into_static(),
            min_ntime: m.min_ntime,
            nbits: m.nbits,
        };
        self.last_prev_hash = Some(cloned.clone());
    }
    fn on_new_extended_mining_job(&mut self, m: NewExtendedMiningJob<'static>) {
        self.last_received_job = Some(m.clone());
        if m.future_job {
            self.future_jobs.push(m)
        } else {
            self.last_valid_job = Some(m)
        }
    }
    fn last_received_job_to_standard_job(
        &mut self,
        channel_id: u32,
    ) -> Result<NewMiningJob<'static>, Error> {
        match &self.last_received_job {
            Some(m) => {
                let downstream = self
                    .hom_downstreams
                    .get(&channel_id)
                    .ok_or(Error::NotFoundChannelId)?;
                extended_to_standard_job(
                    m,
                    &downstream.extranonce.clone().to_vec(),
                    downstream.channel_id,
                    None,
                )
                .ok_or(Error::ImpossibleToCalculateMerkleRoot)
            }
            None => Err(Error::NoValidJob),
        }
    }
}
