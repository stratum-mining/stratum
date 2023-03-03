use super::extended_to_standard_job;
use crate::{
    common_properties::StandardChannel,
    job_creator::JobsCreators,
    parsers::Mining,
    utils::{GroupId, Id, Mutex},
    Error,
};

use mining_sv2::{
    ExtendedExtranonce, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannelSuccess,
    OpenMiningChannelError, OpenStandardMiningChannelSuccess, SetCustomMiningJob,
    SetCustomMiningJobSuccess, SetNewPrevHash, SubmitSharesError, SubmitSharesExtended,
    SubmitSharesStandard, Target,
};

use std::{collections::HashMap, convert::TryInto, sync::Arc};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashFromTp};

use bitcoin::{
    hashes::{sha256d::Hash, Hash as Hash_},
    TxOut,
};
use tracing::error;

pub struct PartialSetCustomMiningJob {
    pub version: u32,
    pub prev_hash: binary_sv2::U256<'static>,
    pub min_ntime: u32,
    pub nbits: u32,
    pub coinbase_tx_version: u32,
    pub coinbase_prefix: binary_sv2::B0255<'static>,
    pub coinbase_tx_input_n_sequence: u32,
    pub coinbase_tx_value_remaining: u64,
    pub coinbase_tx_outputs: binary_sv2::B064K<'static>,
    pub coinbase_tx_locktime: u32,
    pub merkle_path: binary_sv2::Seq0255<'static, binary_sv2::U256<'static>>,
    pub extranonce_size: u16,
    pub future_job: bool,
}

/// Rapresent the action that needs to be done when a new share is received.
#[derive(Debug, Clone)]
pub enum OnNewShare {
    /// Used when the received is malformed, is for an inexistent channel or do not meet downstream
    /// target.
    SendErrorDownstream(SubmitSharesError<'static>),
    /// Used when an exteded channel in a proxy receive a share, and the share meet upstream
    /// target, in this case a new share must be sent upstream.
    SendSubmitShareUpstream(Share),
    /// Used when a group channel in a proxy receive a share that is not malformed and is for a
    /// valid channel in that case we relay the same exact share upstream with a new request id.
    RelaySubmitShareUpstream,
    /// Indicate that the share meet bitcoin target, when there is an upstream the we should send
    /// the share upstream, whenever possible we should also notify the TP about it.
    /// When a pool negotiate a job with downstream we do not have the template_id so we set it to
    /// None
    /// (share, template id, coinbase)
    ShareMeetBitcoinTarget((Share, Option<u64>, Vec<u8>)),
    /// Indicate that the share meet downstream target, in the case we could send a success
    /// response dowmstream.
    ShareMeetDownstreamTarget,
}

impl OnNewShare {
    pub fn into_extended(&mut self, extranonce: Vec<u8>, up_id: u32) {
        match self {
            OnNewShare::SendErrorDownstream(_) => (),
            OnNewShare::SendSubmitShareUpstream(share) => match share {
                Share::Extended(_) => (),
                Share::Standard((share, _)) => {
                    let share = SubmitSharesExtended {
                        channel_id: up_id,
                        sequence_number: share.sequence_number,
                        job_id: share.job_id,
                        nonce: share.nonce,
                        ntime: share.ntime,
                        version: share.version,
                        extranonce: extranonce.try_into().unwrap(),
                    };
                    *self = Self::SendSubmitShareUpstream(Share::Extended(share));
                }
            },
            OnNewShare::RelaySubmitShareUpstream => (),
            OnNewShare::ShareMeetBitcoinTarget((share, t_id, coinbase)) => match share {
                Share::Extended(_) => (),
                Share::Standard((share, _)) => {
                    let share = SubmitSharesExtended {
                        channel_id: up_id,
                        sequence_number: share.sequence_number,
                        job_id: share.job_id,
                        nonce: share.nonce,
                        ntime: share.ntime,
                        version: share.version,
                        extranonce: extranonce.try_into().unwrap(),
                    };
                    *self = Self::ShareMeetBitcoinTarget((
                        Share::Extended(share),
                        *t_id,
                        coinbase.clone(),
                    ));
                }
            },
            OnNewShare::ShareMeetDownstreamTarget => todo!(),
        }
    }
}

/// A share can be both extended or standard
#[derive(Clone, Debug)]
pub enum Share {
    Extended(SubmitSharesExtended<'static>),
    // share, group id
    Standard((SubmitSharesStandard, u32)),
}

#[derive(Clone, Debug)]
pub struct StagedPhash {
    job_id: u32,
    prev_hash: binary_sv2::U256<'static>,
    min_ntime: u32,
    nbits: u32,
}

impl StagedPhash {
    pub fn into_set_p_hash(
        &self,
        channel_id: u32,
        new_job_id: Option<u32>,
    ) -> SetNewPrevHash<'static> {
        SetNewPrevHash {
            channel_id,
            job_id: new_job_id.unwrap_or(self.job_id),
            prev_hash: self.prev_hash.clone(),
            min_ntime: self.min_ntime,
            nbits: self.nbits,
        }
    }
}

impl Share {
    pub fn get_sequence_number(&self) -> u32 {
        match self {
            Share::Extended(s) => s.sequence_number,
            Share::Standard(s) => s.0.sequence_number,
        }
    }
    pub fn get_channel_id(&self) -> u32 {
        match self {
            Share::Extended(s) => s.channel_id,
            Share::Standard(s) => s.0.channel_id,
        }
    }
    pub fn get_n_time(&self) -> u32 {
        match self {
            Share::Extended(s) => s.ntime,
            Share::Standard(s) => s.0.ntime,
        }
    }
    pub fn get_nonce(&self) -> u32 {
        match self {
            Share::Extended(s) => s.nonce,
            Share::Standard(s) => s.0.nonce,
        }
    }

    pub fn get_job_id(&self) -> u32 {
        match self {
            Share::Extended(s) => s.job_id,
            Share::Standard(s) => s.0.job_id,
        }
    }

    pub fn get_version(&self) -> u32 {
        match self {
            Share::Extended(s) => s.version,
            Share::Standard(s) => s.0.version,
        }
    }
}

#[derive(Debug)]
/// Basic logic shared between all the channel factory.
struct ChannelFactory {
    ids: Arc<Mutex<GroupId>>,
    standard_channels_for_non_hom_downstreams: HashMap<u64, StandardChannel>,
    standard_channels_for_hom_downstreams: HashMap<u32, StandardChannel>,
    extended_channels: HashMap<u32, OpenExtendedMiningChannelSuccess<'static>>,
    extranonces: ExtendedExtranonce,
    share_per_min: f32,
    // (NewExtendedMiningJob,group ids that already received the future job)
    future_jobs: Vec<(NewExtendedMiningJob<'static>, Vec<u32>)>,
    // (SetNewPrevHash,group ids that already received the set prev_hash)
    last_prev_hash: Option<(StagedPhash, Vec<u32>)>,
    last_prev_hash_: Option<bitcoin::hash_types::BlockHash>,
    // (NewExtendedMiningJob,group ids that already received the job)
    last_valid_job: Option<(NewExtendedMiningJob<'static>, Vec<u32>)>,
    kind: ExtendedChannelKind,
    job_ids: Id,
    channel_to_group_id: HashMap<u32, u32>,
    future_templates: HashMap<u32, NewTemplate<'static>>,
}

impl ChannelFactory {
    pub fn add_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        is_header_only: bool,
        id: u32,
    ) -> Result<Vec<Mining>, Error> {
        match is_header_only {
            true => {
                self.new_standard_channel_for_hom_downstream(request_id, downstream_hash_rate, id)
            }
            false => self.new_standard_channel_for_non_hom_downstream(
                request_id,
                downstream_hash_rate,
                id,
            ),
        }
    }

    pub fn new_extended_channel(
        &mut self,
        request_id: u32,
        hash_rate: f32,
        min_extranonce_size: u16,
    ) -> Option<Vec<Mining<'static>>> {
        let extended_channels_group = 0;
        let max_extranonce_size = self.extranonces.get_range2_len() as u16;
        if min_extranonce_size <= max_extranonce_size {
            // SECURITY is very unlikely to finish the ids btw this unwrap could be used by an attaccher that
            // want to dirsrupt the service maybe we should have a method to reuse ids that are no
            // longer connected?
            let channel_id = self
                .ids
                .safe_lock(|ids| ids.new_channel_id(extended_channels_group))
                .unwrap();
            self.channel_to_group_id.insert(channel_id, 0);
            let target = crate::utils::hash_rate_to_target(hash_rate, self.share_per_min);
            let extranonce = self
                .extranonces
                .next_extended(max_extranonce_size as usize)?;
            let extranonce_prefix = extranonce.into_prefix(self.extranonces.get_prefix_len())?;
            let success = OpenExtendedMiningChannelSuccess {
                request_id,
                channel_id,
                target,
                extranonce_size: max_extranonce_size,
                extranonce_prefix,
            };
            self.extended_channels.insert(channel_id, success.clone());
            let mut result = vec![Mining::OpenExtendedMiningChannelSuccess(success)];
            if let Some((new_prev_hash, _)) = &self.last_prev_hash {
                let new_prev_hash = new_prev_hash.into_set_p_hash(channel_id, None);
                result.push(Mining::SetNewPrevHash(new_prev_hash.clone()))
            };
            if let Some((job, _)) = &self.last_valid_job {
                result.push(Mining::NewExtendedMiningJob(job.clone()))
            };
            for (job, _) in &self.future_jobs {
                result.push(Mining::NewExtendedMiningJob(job.clone()))
            }
            Some(result)
        } else {
            Some(vec![Mining::OpenMiningChannelError(
                OpenMiningChannelError::new_max_target_out_of_range(request_id),
            )])
        }
    }

    fn new_standard_channel_for_hom_downstream(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        id: u32,
    ) -> Result<Vec<Mining>, Error> {
        let hom_group_id = 0;
        let mut result = vec![];
        let channel_id = id;
        let target = crate::utils::hash_rate_to_target(downstream_hash_rate, self.share_per_min);
        let extranonce = self
            .extranonces
            .next_standard()
            .ok_or(Error::ExtranonceSpaceEnded)?;
        let standard_channel = StandardChannel {
            channel_id,
            group_id: hom_group_id,
            target: target.clone().into(),
            extranonce: extranonce.clone(),
        };
        self.standard_channels_for_hom_downstreams
            .insert(channel_id, standard_channel);

        // First message to be sent is OpenStandardMiningChannelSuccess
        result.push(Mining::OpenStandardMiningChannelSuccess(
            OpenStandardMiningChannelSuccess {
                request_id: request_id.into(),
                channel_id,
                target,
                extranonce_prefix: extranonce.into(),
                group_channel_id: hom_group_id,
            },
        ));
        self.prepare_standard_jobs_and_p_hash(&mut result, channel_id)?;
        self.channel_to_group_id.insert(channel_id, hom_group_id);
        Ok(result)
    }

    // This function is called when downstream have a group channel
    fn new_standard_channel_for_non_hom_downstream(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        group_id: u32,
    ) -> Result<Vec<Mining>, Error> {
        let mut result = vec![];
        let channel_id = self
            .ids
            .safe_lock(|ids| ids.new_channel_id(group_id))
            .unwrap();
        let complete_id = GroupId::into_complete_id(group_id, channel_id);
        let target = crate::utils::hash_rate_to_target(downstream_hash_rate, self.share_per_min);
        let extranonce = self
            .extranonces
            .next_standard()
            .ok_or(Error::ExtranonceSpaceEnded)?;
        let standard_channel = StandardChannel {
            channel_id,
            group_id,
            target: target.clone().into(),
            extranonce: extranonce.clone(),
        };
        self.standard_channels_for_non_hom_downstreams
            .insert(complete_id, standard_channel);

        // First message to be sent is OpenStandardMiningChannelSuccess
        result.push(Mining::OpenStandardMiningChannelSuccess(
            OpenStandardMiningChannelSuccess {
                request_id: request_id.into(),
                channel_id,
                target,
                extranonce_prefix: extranonce.into(),
                group_channel_id: group_id,
            },
        ));
        self.prepare_jobs_and_p_hash(&mut result, complete_id);
        self.channel_to_group_id.insert(channel_id, group_id);
        Ok(result)
    }

    // When a hom dowmstream open a channel we use this function to prepare all the standard jobs
    // (future and not) that we need to send downstream
    fn prepare_standard_jobs_and_p_hash(
        &mut self,
        result: &mut Vec<Mining>,
        channel_id: u32,
    ) -> Result<(), Error> {
        // Safe cause the function is private and we always add the channel before calling this
        // funtion
        let standard_channel = self
            .standard_channels_for_hom_downstreams
            .get(&channel_id)
            .unwrap();
        // OPTIMIZATION this could be memoized somewhere cause is very likely that we will receive a lot od
        // OpenStandardMiningChannel requests consequtevely
        let job_id = self.job_ids.next();
        let future_jobs: Option<Vec<NewMiningJob<'static>>> = self
            .future_jobs
            .iter()
            .map(|j| {
                extended_to_standard_job(
                    &j.0,
                    &standard_channel.extranonce.clone().to_vec()[..],
                    standard_channel.channel_id,
                    Some(job_id),
                )
            })
            .collect();

        // OPTIMIZATION the extranonce is cloned so many time but maybe is avoidable?
        let last_valid_job = match &self.last_valid_job {
            Some((j, _)) => Some(
                extended_to_standard_job(
                    j,
                    &standard_channel.extranonce.clone().to_vec(),
                    standard_channel.channel_id,
                    Some(self.job_ids.next()),
                )
                .ok_or(Error::ImpossibleToCalculateMerkleRoot)?,
            ),
            None => None,
        };

        // This is the same thing of just check if there is a prev hash add it to result. If there
        // is last_job add it to result and add each future job to result.
        // But using the pattern match is more clear how each option is handled
        match (
            &self.last_prev_hash,
            last_valid_job,
            self.future_jobs.is_empty(),
        ) {
            // If we do not have anything just do nothing
            (None, None, true) => Ok(()),
            // If we have only future jobs we need to send them all after the
            // SetupConnectionSuccess message
            (None, None, false) => {
                // Safe unwrap cause we check that self.future_jobs is not empty
                let mut future_jobs = future_jobs.unwrap();
                while let Some(job) = future_jobs.pop() {
                    result.push(Mining::NewMiningJob(job));
                }
                Ok(())
            }
            // If we have just a prev hash we need to send it after the SetupConnectionSuccess
            // message
            (Some((prev_h, _)), None, true) => {
                let prev_h = prev_h.into_set_p_hash(channel_id, None);
                result.push(Mining::SetNewPrevHash(prev_h.clone()));
                Ok(())
            }
            // If we have a prev hash and a last valid job we need to send new mining job before the prev hash
            (Some((prev_h, _)), Some(job), true) => {
                let prev_h = prev_h.into_set_p_hash(channel_id, Some(job.job_id));

                // set future_job to true
                let future_job = NewMiningJob {
                    future_job: true,
                    ..job.clone()
                };

                result.push(Mining::NewMiningJob(future_job));
                result.push(Mining::SetNewPrevHash(prev_h.clone()));
                Ok(())
            }
            // If we have everything we need, send the future jobs and the the prev hash
            (Some((prev_h, _)), Some(job), false) => {
                let prev_h = prev_h.into_set_p_hash(channel_id, Some(job.job_id));

                // set future_job to true
                let future_job = NewMiningJob {
                    future_job: true,
                    ..job.clone()
                };

                result.push(Mining::NewMiningJob(future_job));
                result.push(Mining::SetNewPrevHash(prev_h.clone()));

                // Safe unwrap cause we check that self.future_jobs is not empty
                let mut future_jobs = future_jobs.unwrap();

                while let Some(job) = future_jobs.pop() {
                    result.push(Mining::NewMiningJob(job));
                }
                Ok(())
            }
            // This can not happen because we can not have a valid job without a prev hash
            (None, Some(_), true) => unreachable!(),
            // This can not happen because we can not have a valid job without a prev hash
            (None, Some(_), false) => unreachable!(),
            // This can not happen because as soon as a prev hash is received we flush the future
            // jobs
            (Some(_), None, false) => unreachable!(),
        }
    }

    // When a new non HOM downstreams open a channel we use this function to prepare all the
    // extended job (future and non) and the prev hash that we need to send dowmstream
    fn prepare_jobs_and_p_hash(&mut self, result: &mut Vec<Mining>, complete_id: u64) {
        // If group is 0 it means that we are preparing jobs and p hash for a non HOM downstream
        // that want to open a new extended channel in that case we want to use the channel id
        // TODO verify that this is true also for the case where the channle factory is in a proxy
        // and not in a pool.
        let group_id = match GroupId::into_group_id(complete_id) {
            0 => GroupId::into_channel_id(complete_id),
            a => a,
        };
        // This is the same thing of just check if there is a prev hash add it to result if there
        // is last_job add it to result and add each future job to result.
        // But using the pattern match is more clear how each option is handled
        match (
            self.last_prev_hash.as_mut(),
            self.last_valid_job.as_mut(),
            self.future_jobs.is_empty(),
        ) {
            // If we do not have anything just do nothing
            (None, None, true) => (),
            // If we have only future jobs we need to send them all after the
            // SetupConnectionSuccess message
            (None, None, false) => {
                for (job, group_id_job_sent) in &mut self.future_jobs {
                    if !group_id_job_sent.contains(&group_id) {
                        let mut job = job.clone();
                        job.channel_id = group_id;
                        group_id_job_sent.push(group_id);
                        result.push(Mining::NewExtendedMiningJob(job));
                    }
                }
            }
            // If we have just a prev hash we need to send it after the SetupConnectionSuccess
            // message
            (Some((prev_h, group_id_p_hash_sent)), None, true) => {
                if !group_id_p_hash_sent.contains(&group_id) {
                    let prev_h = prev_h.into_set_p_hash(group_id, None);
                    group_id_p_hash_sent.push(group_id);
                    result.push(Mining::SetNewPrevHash(prev_h.clone()));
                }
            }
            // If we have a prev hash and a last valid job we need to send before the prev hash and
            // the the valid job
            (Some((prev_h, group_id_p_hash_sent)), Some((job, group_id_job_sent)), true) => {
                if !group_id_p_hash_sent.contains(&group_id) {
                    let prev_h = prev_h.into_set_p_hash(group_id, Some(job.job_id));
                    group_id_p_hash_sent.push(group_id);
                    result.push(Mining::SetNewPrevHash(prev_h));
                }
                if !group_id_job_sent.contains(&group_id) {
                    let mut job = job.clone();
                    job.channel_id = group_id;
                    group_id_job_sent.push(group_id);
                    result.push(Mining::NewExtendedMiningJob(job));
                }
            }
            // If we have everything we need, send before the prev hash and then all the jobs
            (Some((prev_h, group_id_p_hash_sent)), Some((job, group_id_job_sent)), false) => {
                if !group_id_p_hash_sent.contains(&group_id) {
                    let prev_h = prev_h.into_set_p_hash(group_id, Some(job.job_id));
                    group_id_p_hash_sent.push(group_id);
                    result.push(Mining::SetNewPrevHash(prev_h));
                }

                if !group_id_job_sent.contains(&group_id) {
                    let mut job = job.clone();
                    job.channel_id = group_id;
                    group_id_job_sent.push(group_id);
                    result.push(Mining::NewExtendedMiningJob(job));
                }

                for (job, group_id_future_j_sent) in &mut self.future_jobs {
                    if !group_id_future_j_sent.contains(&group_id) {
                        let mut job = job.clone();
                        job.channel_id = group_id;
                        group_id_future_j_sent.push(group_id);
                        result.push(Mining::NewExtendedMiningJob(job));
                    }
                }
            }
            // This can not happen because we can not have a valid job without a prev hash
            (None, Some(_), true) => unreachable!(),
            // This can not happen because we can not have a valid job without a prev hash
            (None, Some(_), false) => unreachable!(),
            // This can not happen because as soon as a prev hash is received we flush the future
            // jobs
            (Some(_), None, false) => unreachable!(),
        }
    }

    fn on_new_prev_hash(&mut self, m: StagedPhash) -> Result<(), Error> {
        while let Some(mut job) = self.future_jobs.pop() {
            if job.0.job_id == m.job_id {
                job.0.future_job = false;
                self.last_valid_job = Some(job);
                break;
            }
            self.last_valid_job = None;
        }
        self.future_jobs = vec![];
        self.last_prev_hash_ = Some(crate::utils::u256_to_block_hash(m.prev_hash.clone()));
        let mut ids = vec![];
        for complete_id in self.standard_channels_for_non_hom_downstreams.keys() {
            let group_id = GroupId::into_group_id(*complete_id);
            if !ids.contains(&group_id) {
                ids.push(group_id)
            }
        }
        self.last_prev_hash = Some((m, ids));
        Ok(())
    }

    fn on_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob<'static>,
    ) -> Result<HashMap<u32, Mining<'static>>, Error> {
        match (m.future_job, &self.last_prev_hash) {
            (true, _) => {
                let mut result = HashMap::new();
                self.prepare_jobs_for_downstream_on_new_extended(&mut result, &m)?;
                let mut ids = vec![];
                for complete_id in self.standard_channels_for_non_hom_downstreams.keys() {
                    let group_id = GroupId::into_group_id(*complete_id);
                    if !ids.contains(&group_id) {
                        ids.push(group_id)
                    }
                }
                self.future_jobs.push((m, ids));
                Ok(result)
            }
            (false, Some(_)) => {
                let mut result = HashMap::new();
                self.prepare_jobs_for_downstream_on_new_extended(&mut result, &m)?;
                // If job is not future it must always be paired with the last received prev hash
                let mut ids = vec![];
                for complete_id in self.standard_channels_for_non_hom_downstreams.keys() {
                    let group_id = GroupId::into_group_id(*complete_id);
                    if !ids.contains(&group_id) {
                        ids.push(group_id)
                    }
                }
                self.last_valid_job = Some((m, ids));
                if let Some((_p_hash, _)) = &self.last_prev_hash {
                    Ok(result)
                } else {
                    Err(Error::JobIsNotFutureButPrevHashNotPresent)
                }
            }
            // This should not happen when a non future job is received we always need to have a
            // prev hash
            (false, None) => Err(Error::JobIsNotFutureButPrevHashNotPresent),
        }
    }

    // When a new extended job is received we use this function to prepare the jobs to be sent
    // downstream (standard for hom and this job for non hom)
    fn prepare_jobs_for_downstream_on_new_extended(
        &mut self,
        result: &mut HashMap<u32, Mining>,
        m: &NewExtendedMiningJob<'static>,
    ) -> Result<(), Error> {
        for (id, channel) in &self.standard_channels_for_hom_downstreams {
            let job_id = self.job_ids.next();
            let mut standard_job = extended_to_standard_job(
                m,
                &channel.extranonce.clone().to_vec()[..],
                *id,
                Some(job_id),
            )
            .unwrap();
            standard_job.channel_id = *id;
            let standard_job = Mining::NewMiningJob(standard_job);
            result.insert(*id, standard_job);
        }
        for id in self.standard_channels_for_non_hom_downstreams.keys() {
            let group_id = GroupId::into_group_id(*id);
            let mut extended = m.clone();
            extended.channel_id = group_id;
            let extended_job = Mining::NewExtendedMiningJob(extended);
            result.insert(group_id, extended_job);
        }
        for id in self.extended_channels.keys() {
            let mut extended = m.clone();
            extended.channel_id = *id;
            let extended_job = Mining::NewExtendedMiningJob(extended);
            result.insert(*id, extended_job);
        }
        Ok(())
    }

    // If there is job creator  bitocin_target is retreived from there if not is set to 0
    // If there is a job creator we pass the correct template id if not we pass None
    fn check_target(
        &mut self,
        m: Share,
        bitcoin_target: mining_sv2::Target,
        template_id: Option<u64>,
        up_id: u32,
    ) -> Result<OnNewShare, Error> {
        let upstream_target = match &self.kind {
            ExtendedChannelKind::Pool => Target::new(0, 0),
            ExtendedChannelKind::Proxy {
                upstream_target, ..
            }
            | ExtendedChannelKind::ProxyJn {
                upstream_target, ..
            } => upstream_target.clone(),
        };
        let (downstream_target, extranonce) = self
            .get_channel_specific_mining_info(&m)
            .ok_or(Error::ShareDoNotMatchAnyChannel)?;

        let coinbase_tx_prefix = self
            .last_valid_job
            .as_ref()
            .ok_or(Error::ShareDoNotMatchAnyJob)?
            .0
            .coinbase_tx_prefix
            .as_ref();
        let coinbase_tx_suffix = self
            .last_valid_job
            .as_ref()
            .ok_or(Error::ShareDoNotMatchAnyJob)?
            .0
            .coinbase_tx_suffix
            .as_ref();
        let merkle_path = self.last_valid_job.as_ref().unwrap().0.merkle_path.to_vec();
        // Safe unwrap a sha256 can always be converted into [u8;32]
        let merkle_root: [u8; 32] = crate::utils::merkle_root_from_path(
            coinbase_tx_prefix,
            coinbase_tx_suffix,
            &extranonce[..],
            &merkle_path[..],
        )
        .ok_or(Error::InvalidCoinbase)?
        .try_into()
        .unwrap();
        let version: i32 = self
            .last_valid_job
            .as_ref()
            .ok_or(Error::ShareDoNotMatchAnyJob)?
            .0
            .version
            .try_into()
            .map_err(|_| Error::VersionTooBig)?;
        let header = bitcoin::blockdata::block::BlockHeader {
            version,
            prev_blockhash: self.last_prev_hash_.ok_or(Error::ShareDoNotMatchAnyJob)?,
            merkle_root: Hash::from_inner(merkle_root).into(),
            time: m.get_n_time(),
            bits: self
                .last_prev_hash
                .as_ref()
                .ok_or(Error::ShareDoNotMatchAnyJob)?
                .0
                .nbits,
            nonce: m.get_nonce(),
        };
        let hash_ = header.block_hash();
        let hash = hash_.as_hash().into_inner();
        let hash: Target = hash.into();

        if hash <= bitcoin_target {
            let coinbase = [coinbase_tx_prefix, &extranonce[..], coinbase_tx_suffix]
                .concat()
                .to_vec();
            match self.kind {
                ExtendedChannelKind::Proxy { .. } | ExtendedChannelKind::ProxyJn { .. } => {
                    let upstream_extranonce_space = self.extranonces.get_range0_len();
                    let extranonce = extranonce[upstream_extranonce_space..].to_vec();
                    let mut res = OnNewShare::ShareMeetBitcoinTarget((m, template_id, coinbase));
                    res.into_extended(extranonce, up_id);
                    Ok(res)
                }
                ExtendedChannelKind::Pool => Ok(OnNewShare::ShareMeetBitcoinTarget((
                    m,
                    template_id,
                    coinbase,
                ))),
            }
        } else if hash <= upstream_target {
            match self.kind {
                ExtendedChannelKind::Proxy { .. } | ExtendedChannelKind::ProxyJn { .. } => {
                    let upstream_extranonce_space = self.extranonces.get_range0_len();
                    let extranonce = extranonce[upstream_extranonce_space..].to_vec();
                    let mut res = OnNewShare::SendSubmitShareUpstream(m);
                    res.into_extended(extranonce, up_id);
                    Ok(res)
                }
                ExtendedChannelKind::Pool => Ok(OnNewShare::SendSubmitShareUpstream(m)),
            }
        } else if hash <= downstream_target {
            Ok(OnNewShare::ShareMeetDownstreamTarget)
        } else {
            let error = SubmitSharesError {
                channel_id: m.get_channel_id(),
                sequence_number: m.get_sequence_number(),
                // Infallible unwrap we already know the len of the error code (is a
                // static string)
                error_code: SubmitSharesError::difficulty_too_low_error_code()
                    .to_string()
                    .try_into()
                    .unwrap(),
            };
            Ok(OnNewShare::SendErrorDownstream(error))
        }
    }

    fn get_channel_specific_mining_info(&self, m: &Share) -> Option<(mining_sv2::Target, Vec<u8>)> {
        match m {
            Share::Extended(share) => {
                let channel = self.extended_channels.get(&m.get_channel_id())?;
                let extranonce_prefix = channel.extranonce_prefix.to_vec();
                let dowstream_target = channel.target.clone().into();
                let extranonce = [&extranonce_prefix[..], &share.extranonce.to_vec()[..]]
                    .concat()
                    .to_vec();
                if extranonce.len() != self.extranonces.get_len() {
                    error!(
                        "Extranonce is not of the right len expected {} actual {}",
                        self.extranonces.get_len(),
                        extranonce.len()
                    );
                }
                Some((dowstream_target, extranonce))
            }
            Share::Standard((share, group_id)) => match &self.kind {
                ExtendedChannelKind::Pool => {
                    let complete_id = GroupId::into_complete_id(*group_id, share.channel_id);
                    let mut channel = self
                        .standard_channels_for_non_hom_downstreams
                        .get(&complete_id);
                    if channel.is_none() {
                        channel = self
                            .standard_channels_for_hom_downstreams
                            .get(&share.channel_id);
                    };
                    Some((
                        channel?.target.clone(),
                        channel?.extranonce.clone().to_vec(),
                    ))
                }
                ExtendedChannelKind::Proxy { .. } | ExtendedChannelKind::ProxyJn { .. } => {
                    let complete_id = GroupId::into_complete_id(*group_id, share.channel_id);
                    let mut channel = self
                        .standard_channels_for_non_hom_downstreams
                        .get(&complete_id);
                    if channel.is_none() {
                        channel = self
                            .standard_channels_for_hom_downstreams
                            .get(&share.channel_id);
                    };
                    Some((
                        channel?.target.clone(),
                        channel?.extranonce.clone().to_vec(),
                    ))
                }
            },
        }
    }
}

/// Used by a pool to in order to manage all downstream channel. It add job creation capabilities
/// to ChannelFactory.
#[derive(Debug)]
pub struct PoolChannelFactory {
    inner: ChannelFactory,
    job_creator: JobsCreators,
    pool_coinbase_outputs: Vec<TxOut>,
    // extedned_channel_id -> SetCustomMiningJob
    negotiated_jobs: HashMap<u32, SetCustomMiningJob<'static>>,
}

impl PoolChannelFactory {
    pub fn new(
        ids: Arc<Mutex<GroupId>>,
        extranonces: ExtendedExtranonce,
        job_creator: JobsCreators,
        share_per_min: f32,
        kind: ExtendedChannelKind,
        pool_coinbase_outputs: Vec<TxOut>,
    ) -> Self {
        let inner = ChannelFactory {
            ids,
            standard_channels_for_non_hom_downstreams: HashMap::new(),
            standard_channels_for_hom_downstreams: HashMap::new(),
            extended_channels: HashMap::new(),
            extranonces,
            share_per_min,
            future_jobs: Vec::new(),
            last_prev_hash: None,
            last_prev_hash_: None,
            last_valid_job: None,
            kind,
            job_ids: Id::new(),
            channel_to_group_id: HashMap::new(),
            future_templates: HashMap::new(),
        };

        Self {
            inner,
            job_creator,
            pool_coinbase_outputs,
            negotiated_jobs: HashMap::new(),
        }
    }

    pub fn add_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        is_header_only: bool,
        id: u32,
    ) -> Result<Vec<Mining>, Error> {
        self.inner
            .add_standard_channel(request_id, downstream_hash_rate, is_header_only, id)
    }

    pub fn new_extended_channel(
        &mut self,
        request_id: u32,
        hash_rate: f32,
        min_extranonce_size: u16,
    ) -> Option<Vec<Mining<'static>>> {
        self.inner
            .new_extended_channel(request_id, hash_rate, min_extranonce_size)
    }

    pub fn on_new_prev_hash_from_tp(
        &mut self,
        m: &SetNewPrevHashFromTp<'static>,
    ) -> Result<u32, Error> {
        let job_id = self.job_creator.on_new_prev_hash(m).unwrap_or(0);
        let new_prev_hash = StagedPhash {
            job_id,
            prev_hash: m.prev_hash.clone(),
            min_ntime: m.header_timestamp,
            nbits: m.n_bits,
        };
        self.inner.on_new_prev_hash(new_prev_hash)?;
        Ok(job_id)
    }

    pub fn on_new_template(
        &mut self,
        m: &mut NewTemplate<'static>,
    ) -> Result<HashMap<u32, Mining<'static>>, Error> {
        // edit the last pool_coinbase_output
        if let Some(last_pool_coinbase_output) = self.pool_coinbase_outputs.last_mut() {
            last_pool_coinbase_output.value = m.coinbase_tx_value_remaining;
        }

        let new_job =
            self.job_creator
                .on_new_template(m, true, self.pool_coinbase_outputs.clone())?;
        self.inner.on_new_extended_mining_job(new_job)
    }

    pub fn on_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<OnNewShare, Error> {
        match self.inner.channel_to_group_id.get(&m.channel_id) {
            Some(g_id) => {
                let template_id = self
                    .job_creator
                    .get_template_id_from_job(self.inner.last_valid_job.as_ref().unwrap().0.job_id)
                    .ok_or(Error::NoTemplateForId)?;
                let target = self.job_creator.last_target();
                self.inner
                    .check_target(Share::Standard((m, *g_id)), target, Some(template_id), 0)
            }
            None => {
                let err = SubmitSharesError {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    error_code: SubmitSharesError::invalid_channel_error_code()
                        .to_string()
                        .try_into()
                        .unwrap(),
                };
                Ok(OnNewShare::SendErrorDownstream(err))
            }
        }
    }

    pub fn on_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<OnNewShare, Error> {
        let target = self.job_creator.last_target();
        // When downstream set a custom mining job we add the the job to the negotiated job
        // hashmap, with the extended channel id as a key. Whenever the pool receive a share must
        // first check if the channel have a negotiated job if so we can not retreive the template
        // via the job creator (TODO MVP3 add a way to get the template for negotiated job create a
        // block from the template and send to bitcoind via RPC).
        if self.negotiated_jobs.contains_key(&m.channel_id) {
            self.inner
                .check_target(Share::Extended(m.into_static()), target, None, 0)
        } else {
            let template_id = self
                .job_creator
                .get_template_id_from_job(self.inner.last_valid_job.as_ref().unwrap().0.job_id)
                .ok_or(Error::NoTemplateForId)?;
            self.inner.check_target(
                Share::Extended(m.into_static()),
                target,
                Some(template_id),
                0,
            )
        }
    }

    pub fn new_group_id(&mut self) -> u32 {
        let new_id = self.inner.ids.safe_lock(|ids| ids.new_group_id()).unwrap();
        new_id
    }
    pub fn new_standard_id_for_hom(&mut self) -> u32 {
        let hom_group_id = 0;
        let new_id = self
            .inner
            .ids
            .safe_lock(|ids| ids.new_channel_id(hom_group_id))
            .unwrap();
        new_id
    }

    pub fn extranonce_from_downstream_extranonce(
        &self,
        ext: mining_sv2::Extranonce,
    ) -> Option<mining_sv2::Extranonce> {
        self.inner
            .extranonces
            .extranonce_from_downstream_extranonce(ext)
    }

    pub fn on_new_set_custom_mining_job(
        &mut self,
        set_custom_mining_job: SetCustomMiningJob<'static>,
    ) -> SetCustomMiningJobSuccess {
        if self.check_set_custom_mining_job(&set_custom_mining_job) {
            self.negotiated_jobs.insert(
                set_custom_mining_job.channel_id,
                set_custom_mining_job.clone(),
            );
            SetCustomMiningJobSuccess {
                channel_id: set_custom_mining_job.channel_id,
                request_id: set_custom_mining_job.request_id,
                job_id: 0,
            }
        } else {
            todo!()
        }
    }

    fn check_set_custom_mining_job(
        &self,
        _set_custom_mining_job: &SetCustomMiningJob<'static>,
    ) -> bool {
        true
    }
}

/// Used by proxies that want to open extended channls with upstream. If the proxy have job
/// negotiation capabilities we set the job creator and the coinbase outs.
#[derive(Debug)]
pub struct ProxyExtendedChannelFactory {
    inner: ChannelFactory,
    job_creator: Option<JobsCreators>,
    pool_coinbase_outputs: Option<Vec<TxOut>>,
    // Id assigned to the extended channel by upstream
    extended_channel_id: u32,
}

impl ProxyExtendedChannelFactory {
    pub fn new(
        ids: Arc<Mutex<GroupId>>,
        extranonces: ExtendedExtranonce,
        job_creator: Option<JobsCreators>,
        share_per_min: f32,
        kind: ExtendedChannelKind,
        pool_coinbase_outputs: Option<Vec<TxOut>>,
        extended_channel_id: u32,
    ) -> Self {
        match &kind {
            ExtendedChannelKind::Proxy { .. } => {
                if job_creator.is_some() {
                    panic!("Channel factory of kind Proxy can not be initialized with a JobCreators");
                };
            },
            ExtendedChannelKind::ProxyJn { .. } => {
                if job_creator.is_none() {
                    panic!("Channel factory of kind ProxyJn must be initialized with a JobCreators");
                };
            }
            ExtendedChannelKind::Pool => panic!("Try to construct an ProxyExtendedChannelFactory with pool kind, kind must be Proxy or ProxyJn"),
        };
        let inner = ChannelFactory {
            ids,
            standard_channels_for_non_hom_downstreams: HashMap::new(),
            standard_channels_for_hom_downstreams: HashMap::new(),
            extended_channels: HashMap::new(),
            extranonces,
            share_per_min,
            future_jobs: Vec::new(),
            last_prev_hash: None,
            last_prev_hash_: None,
            last_valid_job: None,
            kind,
            job_ids: Id::new(),
            channel_to_group_id: HashMap::new(),
            future_templates: HashMap::new(),
        };
        ProxyExtendedChannelFactory {
            inner,
            job_creator,
            pool_coinbase_outputs,
            extended_channel_id,
        }
    }

    pub fn add_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hash_rate: f32,
        id_header_only: bool,
        id: u32,
    ) -> Result<Vec<Mining>, Error> {
        self.inner
            .add_standard_channel(request_id, downstream_hash_rate, id_header_only, id)
    }

    pub fn new_extended_channel(
        &mut self,
        request_id: u32,
        hash_rate: f32,
        min_extranonce_size: u16,
    ) -> Option<Vec<Mining>> {
        self.inner
            .new_extended_channel(request_id, hash_rate, min_extranonce_size)
    }

    pub fn on_new_prev_hash_from_tp(
        &mut self,
        m: &SetNewPrevHashFromTp<'static>,
    ) -> Result<Option<PartialSetCustomMiningJob>, Error> {
        if let Some(job_creator) = self.job_creator.as_mut() {
            let job_id = job_creator.on_new_prev_hash(m).unwrap_or(0);
            let new_prev_hash = StagedPhash {
                job_id,
                prev_hash: m.prev_hash.clone(),
                min_ntime: m.header_timestamp,
                nbits: m.n_bits,
            };
            let mut custom_job = None;
            if let Some(template) = self.inner.future_templates.get(&job_id) {
                custom_job = Some(PartialSetCustomMiningJob {
                    version: template.version,
                    prev_hash: new_prev_hash.prev_hash.clone(),
                    min_ntime: new_prev_hash.min_ntime,
                    nbits: new_prev_hash.nbits,
                    coinbase_tx_version: template.coinbase_tx_version,
                    coinbase_prefix: template.coinbase_prefix.clone(),
                    coinbase_tx_input_n_sequence: template.coinbase_tx_input_sequence,
                    coinbase_tx_value_remaining: template.coinbase_tx_value_remaining,
                    coinbase_tx_outputs: template.coinbase_tx_outputs.clone(),
                    coinbase_tx_locktime: template.coinbase_tx_locktime,
                    merkle_path: template.merkle_path.clone(),
                    extranonce_size: self.inner.extranonces.get_len() as u16,
                    future_job: template.future_template,
                });
            }
            self.inner.future_templates = HashMap::new();
            self.inner.on_new_prev_hash(new_prev_hash)?;
            Ok(custom_job)
        } else {
            panic!("A channel factory without job creator do not have negotiation capabilities")
        }
    }

    pub fn on_new_template(
        &mut self,
        m: &mut NewTemplate<'static>,
    ) -> Result<
        (
            HashMap<u32, Mining<'static>>,
            Option<PartialSetCustomMiningJob>,
        ),
        Error,
    > {
        if let (Some(job_creator), Some(pool_coinbase_outputs)) = (
            self.job_creator.as_mut(),
            self.pool_coinbase_outputs.as_ref(),
        ) {
            let new_job = job_creator.on_new_template(m, true, pool_coinbase_outputs.clone())?;
            if !new_job.future_job && self.inner.last_prev_hash.is_some() {
                let prev_hash = self.last_prev_hash().unwrap();
                let min_ntime = self.last_min_ntime().unwrap();
                let nbits = self.last_nbits().unwrap();
                let extranonce_size = self.extranonce_size() as u16;
                let custom_mining_job = PartialSetCustomMiningJob {
                    version: m.version,
                    prev_hash,
                    min_ntime,
                    nbits,
                    coinbase_tx_version: m.coinbase_tx_version,
                    coinbase_prefix: m.coinbase_prefix.clone(),
                    coinbase_tx_input_n_sequence: m.coinbase_tx_input_sequence,
                    coinbase_tx_value_remaining: m.coinbase_tx_value_remaining,
                    coinbase_tx_outputs: m.coinbase_tx_outputs.clone(),
                    coinbase_tx_locktime: m.coinbase_tx_locktime,
                    merkle_path: m.merkle_path.clone(),
                    extranonce_size,
                    future_job: m.future_template,
                };
                return Ok((
                    self.inner.on_new_extended_mining_job(new_job)?,
                    Some(custom_mining_job),
                ));
            }
            if new_job.future_job {
                self.inner
                    .future_templates
                    .insert(new_job.job_id, m.clone());
            }
            Ok((self.inner.on_new_extended_mining_job(new_job)?, None))
        } else {
            panic!("Either channel factory has no job creator or pool_coinbase_outputs are not yet set")
        }
    }

    pub fn on_submit_shares_extended(
        &mut self,
        mut m: SubmitSharesExtended<'static>,
        trim_extranonce: Option<usize>,
    ) -> Result<OnNewShare, Error> {
        if let Some(size) = trim_extranonce {
            let mut extranonce = m.extranonce.to_vec();
            extranonce.reverse();
            for _ in 0..size {
                extranonce.pop();
            }
            extranonce.reverse();
            m.extranonce = extranonce.try_into().unwrap();
        };
        if let Some(job_creator) = self.job_creator.as_mut() {
            let template_id = job_creator
                .get_template_id_from_job(self.inner.last_valid_job.as_ref().unwrap().0.job_id)
                .ok_or(Error::NoTemplateForId)?;
            let bitcoin_target = job_creator.last_target();
            self.inner.check_target(
                Share::Extended(m),
                bitcoin_target,
                Some(template_id),
                self.extended_channel_id,
            )
        } else {
            let bitcoin_target = [0; 32];
            // if there is not job_creator is not proxy duty to check if target is below or above
            // bitcoin target so we set bitcoin_target = 0.
            self.inner.check_target(
                Share::Extended(m),
                bitcoin_target.into(),
                None,
                self.extended_channel_id,
            )
        }
    }

    pub fn on_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<OnNewShare, Error> {
        match self.inner.channel_to_group_id.get(&m.channel_id) {
            Some(g_id) => {
                if let Some(job_creator) = self.job_creator.as_mut() {
                    let template_id = job_creator
                        .get_template_id_from_job(
                            self.inner.last_valid_job.as_ref().unwrap().0.job_id,
                        )
                        .ok_or(Error::NoTemplateForId)?;
                    let bitcoin_target = job_creator.last_target();
                    self.inner.check_target(
                        Share::Standard((m, *g_id)),
                        bitcoin_target,
                        Some(template_id),
                        self.extended_channel_id,
                    )
                } else {
                    let bitcoin_target = [0; 32];
                    // if there is not job_creator is not proxy duty to check if target is below or above
                    // bitcoin target so we set bitcoin_target = 0.
                    self.inner.check_target(
                        Share::Standard((m, *g_id)),
                        bitcoin_target.into(),
                        None,
                        self.extended_channel_id,
                    )
                }
            }
            None => {
                let err = SubmitSharesError {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    error_code: SubmitSharesError::invalid_channel_error_code()
                        .to_string()
                        .try_into()
                        .unwrap(),
                };
                Ok(OnNewShare::SendErrorDownstream(err))
            }
        }
    }
    pub fn on_new_prev_hash(&mut self, m: SetNewPrevHash<'static>) -> Result<(), Error> {
        self.inner.on_new_prev_hash(StagedPhash {
            job_id: m.job_id,
            prev_hash: m.prev_hash.clone().into_static(),
            min_ntime: m.min_ntime,
            nbits: m.nbits,
        })
    }

    pub fn on_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob<'static>,
    ) -> Result<HashMap<u32, Mining<'static>>, Error> {
        self.inner.on_new_extended_mining_job(m)
    }
    pub fn set_target(&mut self, new_target: &mut Target) {
        self.inner.kind.set_target(new_target);
    }
    pub fn last_valid_job_version(&self) -> Option<u32> {
        self.inner.last_valid_job.as_ref().map(|j| j.0.version)
    }
    pub fn extranonce_from_downstream_extranonce(
        &self,
        ext: mining_sv2::Extranonce,
    ) -> Option<mining_sv2::Extranonce> {
        self.inner
            .extranonces
            .extranonce_from_downstream_extranonce(ext)
    }

    pub fn last_prev_hash(&self) -> Option<binary_sv2::U256<'static>> {
        self.inner
            .last_prev_hash
            .as_ref()
            .map(|f| f.0.prev_hash.clone())
    }
    pub fn last_min_ntime(&self) -> Option<u32> {
        self.inner.last_prev_hash.as_ref().map(|f| f.0.min_ntime)
    }
    pub fn last_nbits(&self) -> Option<u32> {
        self.inner.last_prev_hash.as_ref().map(|f| f.0.nbits)
    }
    pub fn extranonce_size(&self) -> usize {
        self.inner.extranonces.get_len()
    }
    pub fn update_pool_outputs(&mut self, outs: Vec<TxOut>) {
        self.pool_coinbase_outputs = Some(outs);
    }

    pub fn get_this_channel_id(&self) -> u32 {
        self.extended_channel_id
    }
}

#[derive(Debug, Clone)]
pub enum ExtendedChannelKind {
    Proxy { upstream_target: Target },
    ProxyJn { upstream_target: Target },
    Pool,
}
impl ExtendedChannelKind {
    pub fn set_target(&mut self, new_target: &mut Target) {
        match self {
            ExtendedChannelKind::Proxy { upstream_target }
            | ExtendedChannelKind::ProxyJn { upstream_target } => {
                std::mem::swap(upstream_target, new_target)
            }
            ExtendedChannelKind::Pool => panic!("Try to set upstream target for a pool"),
        }
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use binary_sv2::{Seq0255, B064K, U256};
    use bitcoin::{hash_types::WPubkeyHash, PublicKey};
    use mining_sv2::OpenStandardMiningChannel;

    const BLOCK_REWARD: u64 = 2_000_000_000;

    // Block 1296 data
    // 01000000
    // c1397d4a33adeeb3383803e9ac3db4b2c2c9d6737cbabc13a534d24600000000
    // 89687b66140ac9874656270e066ed7ef81d5133ada2d0133f09322a87b161738
    // 4eb87749
    // ffff001d
    // 07cacb0e
    const _PUB_K: &str = "04c6d0969c2d98a5c19ba7c36c7937c5edbd60ff2a01397c4afe54f16cd641667ea0049ba6f9e1796ba3c8e49e1b504c532ebbaaa1010c3f7d9b83a8ea7fd800e2";
    const _BLOCK_HASH: &str = "000000009a4aed3e8ba7a978c6b50fea886fb496d66e696090a91d527200b002";
    const VERSION: u32 = 1;
    // version 01000000
    // inputs 01
    // prev out 0000000000000000000000000000000000000000000000000000000000000000ffffffff
    // script len 07
    // script 04ffff001d0177
    // sequence ffffffff
    // n inputs 01
    // amunt 00f2052a01000000
    // out lne 43
    // push 41
    // pub k 04c6d0969c2d98a5c19ba7c36c7937c5edbd60ff2a01397c4afe54f16cd641667ea0049ba6f9e1796ba3c8e49e1b504c532ebbaaa1010c3f7d9b83a8ea7fd800e2
    // checksig ac
    // locktime 00000000
    const COINBASE: &str = "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0177ffffffff0100f2052a01000000434104c6d0969c2d98a5c19ba7c36c7937c5edbd60ff2a01397c4afe54f16cd641667ea0049ba6f9e1796ba3c8e49e1b504c532ebbaaa1010c3f7d9b83a8ea7fd800e2ac00000000";
    const COINBASE_OUTPUT: &str = "4104c6d0969c2d98a5c19ba7c36c7937c5edbd60ff2a01397c4afe54f16cd641667ea0049ba6f9e1796ba3c8e49e1b504c532ebbaaa1010c3f7d9b83a8ea7fd800e2ac";
    const MERKLE_PATH: &str = "59bf8acbc9d60dfae841abecc3882b4181f2bdd8ac6c1d94001165ab3aef50b0";
    const NONCE: &str = "07cacb0e";
    const NTIME: &str = "4eb87749";

    // Prev  block data (1295)
    //01000000
    //cf578a234f330c287354e24234ff6b86d6ab9e4ddd3e5ba71a6bcbf600000000
    //72d12b99bdb63762bedc5db30bcffbd7903721bc736dd683de37b1a3632f9000
    //time: 2e8c7749 -> 49778c2e -> 1232571438
    //nbits: ffff001d -> 4294901789
    //29444816
    const PREV_HASH: &str = "0000000046d234a513bcba7c73d6c9c2b2b43dace9033838b3eead334a7d39c1";
    const PREV_HEADER_TIMESTAMP: u32 = 1232571438;
    const PREV_HEADER_NBITS: u32 = 486604799;

    fn _get_pub_key_hash() -> WPubkeyHash {
        let into_bin = decode_hex(_PUB_K).unwrap();
        let pk = PublicKey::from_slice(&into_bin[..]);
        let hash = pk.unwrap().pubkey_hash();
        WPubkeyHash::from_hash(hash.as_hash())
    }

    fn get_coinbase() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let parsed = decode_hex(COINBASE).unwrap();
        // Coinbase prefix in Sv2 is the bip34 block height in this tx there is no prefix
        let prefix = parsed[42..42].to_vec();
        let extranonce = parsed[42..49].to_vec();
        let suffix = parsed[49..].to_vec();
        (prefix, extranonce, suffix)
    }

    fn get_coinbase_outputs() -> B064K<'static> {
        decode_hex(COINBASE_OUTPUT).unwrap().try_into().unwrap()
    }

    fn get_merkle_path() -> Seq0255<'static, U256<'static>> {
        let mut m_path = decode_hex(MERKLE_PATH).unwrap();
        m_path.reverse();
        let path: U256 = m_path.try_into().unwrap();
        vec![path].try_into().unwrap()
    }

    fn nbit_to_target(nbit: u32) -> U256<'static> {
        let mut target = bitcoin::blockdata::block::BlockHeader::u256_from_compact_target(nbit)
            .to_be_bytes()
            .to_vec();
        target.reverse();
        target.try_into().unwrap()
    }

    fn decode_hex(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    use bitcoin::TxOut;
    use quickcheck::{Arbitrary, Gen};
    use rand::Rng;

    #[test]
    fn test_complete_mining_round() {
        let (prefix, coinbase_extranonce, _) = get_coinbase();

        // Initialize a Channel of type Pool
        let out = TxOut {value: BLOCK_REWARD, script_pubkey: decode_hex("4104c6d0969c2d98a5c19ba7c36c7937c5edbd60ff2a01397c4afe54f16cd641667ea0049ba6f9e1796ba3c8e49e1b504c532ebbaaa1010c3f7d9b83a8ea7fd800e2ac").unwrap().into()};
        let creator = JobsCreators::new(7);
        let share_per_min = 1.0;
        // Create an ExtendedExtranonce of len 7:
        // upstream part is 0 bytes cause we are a pool so no more upstreams
        // self part is 7 bytes
        // downstream part is 0 cause in the test the downstream is HOM so we do not need to
        // reserve space for downstream
        let mut inner = coinbase_extranonce.clone();
        inner[6] = 0;
        let extranonces = ExtendedExtranonce::new_with_inner_only_test(0..0, 0..0, 0..7, inner);

        let ids = Arc::new(Mutex::new(GroupId::new()));
        let channel_kind = ExtendedChannelKind::Pool;
        let mut channel = PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            channel_kind,
            vec![out],
        );

        // Build a NewTemplate
        let mut new_template = NewTemplate {
            template_id: 10,
            future_template: true,
            version: VERSION,
            coinbase_tx_version: 1,
            coinbase_prefix: prefix.try_into().unwrap(),
            coinbase_tx_input_sequence: u32::MAX,
            coinbase_tx_value_remaining: 5_000_000_000,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: get_coinbase_outputs(),
            coinbase_tx_locktime: 0,
            merkle_path: get_merkle_path(),
        };

        // "Send" the NewTemplate to the channel
        let _ = channel.on_new_template(&mut new_template);

        // Build a PrevHash
        let mut p_hash = decode_hex(PREV_HASH).unwrap();
        p_hash.reverse();
        let prev_hash = SetNewPrevHashFromTp {
            template_id: 10,
            prev_hash: p_hash.try_into().unwrap(),
            header_timestamp: PREV_HEADER_TIMESTAMP,
            n_bits: PREV_HEADER_NBITS,
            target: nbit_to_target(PREV_HEADER_NBITS),
        };

        // "Send" the SetNewPrevHash to channel
        let _ = channel.on_new_prev_hash_from_tp(&prev_hash);

        // Build open standard channel
        let open_standard_channel = OpenStandardMiningChannel {
            request_id: 100.into(),
            user_identity: "Gigi".to_string().try_into().unwrap(),
            nominal_hash_rate: 100_000_000_000_000.0,
            max_target: [255; 32].try_into().unwrap(),
        };

        // "Send" the OpenStandardMiningChannel to channel
        let result = loop {
            let id = channel.new_standard_id_for_hom();
            let result = channel
                .add_standard_channel(
                    open_standard_channel.get_request_id_as_u32(),
                    open_standard_channel.nominal_hash_rate,
                    true,
                    id,
                )
                .unwrap();
            let downsteram_extranonce = match &result[0] {
                Mining::OpenStandardMiningChannelSuccess(msg) => {
                    msg.extranonce_prefix.clone().to_vec()
                }
                _ => panic!(),
            };
            if downsteram_extranonce == coinbase_extranonce {
                break result;
            }
        };
        let mut result = result.iter();

        // Get the expected job id and channel_id
        let mut channel_id = u32::MAX;
        let job_id = loop {
            match result.next().unwrap() {
                Mining::OpenStandardMiningChannelSuccess(success) => {
                    channel_id = success.channel_id
                }
                Mining::SetNewPrevHash(_) => (),
                Mining::NewMiningJob(job) => break job.job_id,
                _ => panic!(),
            }
        };

        // Build the success share
        let share = SubmitSharesStandard {
            channel_id,
            sequence_number: 2,
            job_id,
            nonce: u32::from_le_bytes(decode_hex(NONCE).unwrap().try_into().unwrap()),
            ntime: u32::from_le_bytes(decode_hex(NTIME).unwrap().try_into().unwrap()),
            version: 1,
        };

        // "Send" the Share to channel
        match channel.on_submit_shares_standard(share).unwrap() {
            OnNewShare::SendErrorDownstream(e) => panic!(
                "{:?} \n {}",
                e,
                std::str::from_utf8(&e.error_code.to_vec()[..]).unwrap()
            ),
            OnNewShare::SendSubmitShareUpstream(_) => panic!(),
            OnNewShare::RelaySubmitShareUpstream => panic!(),
            OnNewShare::ShareMeetBitcoinTarget(_) => assert!(true),
            OnNewShare::ShareMeetDownstreamTarget => panic!(),
        };
    }
}
