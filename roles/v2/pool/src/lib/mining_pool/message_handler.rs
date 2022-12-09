use crate::lib::mining_pool::{Downstream, VelideateTargetResult};
use binary_sv2::U256;
use bitcoin::util::uint::Uint256;
use roles_logic_sv2::{
    errors::Error,
    handlers::mining::{ParseDownstreamMiningMessages, SendTo, SupportedChannelTypes},
    mining_sv2::*,
    parsers::Mining,
    routing_logic::NoRouting,
    selectors::NullDownstreamMiningSelector,
    utils::{hash_rate_to_target, Mutex},
};
use std::{convert::TryInto, sync::Arc};
use tracing::{debug, trace};

#[allow(clippy::many_single_char_names)]
pub fn u256_to_uint_256(v: U256<'static>) -> Uint256 {
    let bs = v.to_vec();
    let a = u64::from_be_bytes(bs[0..8].try_into().unwrap());
    let b = u64::from_be_bytes(bs[8..16].try_into().unwrap());
    let c = u64::from_be_bytes(bs[16..24].try_into().unwrap());
    let d = u64::from_be_bytes(bs[24..32].try_into().unwrap());
    Uint256([d, c, b, a])
}

impl ParseDownstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Downstream {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
        _m: Option<Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        let request_id = incoming.get_request_id_as_u32();
        let target = hash_rate_to_target(incoming.nominal_hash_rate, 1_f32);
        let extranonce_prefix = self
            .extranonces
            .safe_lock(|e| e.next_standard().unwrap().into_b032())
            .unwrap();

        debug!(
            "Handling open standard mining channel request_id: {} for hash_rate: {}",
            request_id, incoming.nominal_hash_rate
        );

        match (self.downstream_data.header_only, self.id) {
            (false, group_channel_id) => {
                let channel_id = self.channel_ids.next();
                let mut partial_job = crate::lib::mining_pool::Job::new(
                    u256_to_uint_256(target.clone()),
                    extranonce_prefix.clone().to_vec(),
                );
                match (
                    &self.last_valid_extended_job,
                    &self.last_prev_hash,
                    &self.last_nbits,
                ) {
                    (Some(job), Some(p_hash), Some(n_bits)) => {
                        partial_job.update_job(
                            &job.0,
                            *n_bits,
                            *p_hash,
                            job.1,
                            job.0.job_id,
                            job.0.version,
                            job.0.version_rolling_allowed,
                        );
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (None, Some(_), Some(_)) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (None, None, None) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (Some(_), None, None) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (_, Some(_), None) => {
                        panic!("impossible state")
                    }
                    (_, None, Some(_)) => {
                        panic!("impossible state")
                    }
                };

                let message = OpenStandardMiningChannelSuccess {
                    request_id: request_id.into(),
                    channel_id,
                    target,
                    extranonce_prefix,
                    group_channel_id,
                };
                Ok(SendTo::Respond(Mining::OpenStandardMiningChannelSuccess(
                    message,
                )))
            }
            (true, channel_id) => {
                let mut partial_job = crate::lib::mining_pool::Job::new(
                    u256_to_uint_256(target.clone()),
                    extranonce_prefix.clone().to_vec(),
                );
                match (
                    &self.last_valid_extended_job,
                    &self.last_prev_hash,
                    &self.last_nbits,
                ) {
                    (Some(job), Some(p_hash), Some(n_bits)) => {
                        partial_job.update_job(
                            &job.0,
                            *n_bits,
                            *p_hash,
                            job.1,
                            job.0.job_id,
                            job.0.version,
                            job.0.version_rolling_allowed,
                        );
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (None, Some(_), Some(_)) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (None, None, None) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (Some(_), None, None) => {
                        self.jobs.insert(channel_id, partial_job);
                    }
                    (_, Some(_), None) => {
                        panic!("impossible state")
                    }
                    (_, None, Some(_)) => {
                        panic!("impossible state")
                    }
                };

                let mut jobs = vec![];
                let coinbase_script = &extranonce_prefix.clone().to_vec()[..];
                if let Some(job) = self.jobs.get(&channel_id) {
                    let job = match job {
                        super::Job::Complete(job) => job.as_extended(channel_id, false),
                        super::Job::Partial(_) => panic!("IMPOSSIBLE STATE"),
                    };
                    let job = roles_logic_sv2::job_dispatcher::extended_to_standard_job_for_group_channel(&job, coinbase_script, channel_id, job.job_id).unwrap();
                    jobs.push(job)
                };
                for (job, _) in self.future_jobs.values() {
                    let job = roles_logic_sv2::job_dispatcher::extended_to_standard_job_for_group_channel(job, coinbase_script, channel_id, job.job_id).unwrap();
                    jobs.push(job)
                }
                let open_channel = OpenStandardMiningChannelSuccess {
                    request_id: request_id.into(),
                    channel_id,
                    group_channel_id: crate::HOM_GROUP_ID,
                    target,
                    extranonce_prefix: extranonce_prefix.clone(),
                };
                self.extranonce = Some(extranonce_prefix.to_vec());
                let mut jobs = jobs
                    .into_iter()
                    .map(|j| SendTo::Respond(Mining::NewMiningJob(j)))
                    .collect();
                let mut to_send = vec![SendTo::Respond(Mining::OpenStandardMiningChannelSuccess(
                    open_channel.clone(),
                ))];
                to_send.append(&mut jobs);
                Ok(SendTo::Multiple(to_send))
            }
        }
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        incoming: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        if incoming.min_extranonce_size > 16 {
            todo!()
        };
        if self.downstream_data.header_only {
            todo!()
        };
        let request_id = incoming.get_request_id_as_u32();
        let target = hash_rate_to_target(incoming.nominal_hash_rate, 1_f32);
        let extended = self
            .extranonces
            .safe_lock(|e| {
                e.next_extended(incoming.min_extranonce_size as usize)
                    .unwrap()
                    .into_b032()
            })
            .unwrap();
        let channel_id = self.channel_ids.next();
        let mut partial_job = crate::lib::mining_pool::Job::new(
            u256_to_uint_256(target.clone()),
            U256::from(extended.clone()).to_vec(),
        );
        let mut extended = extended.to_vec();
        extended.resize(16, 0);
        self.prefixes.insert(channel_id, extended.clone());
        match (
            &self.last_valid_extended_job,
            &self.last_prev_hash,
            &self.last_nbits,
        ) {
            (Some(job), Some(p_hash), Some(n_bits)) => {
                trace!(
                    "updating job: {:?} {:?} {:?} {:?}",
                    job,
                    n_bits,
                    p_hash,
                    extended
                );
                partial_job.update_job(
                    &job.0,
                    *n_bits,
                    *p_hash,
                    job.1,
                    job.0.job_id,
                    job.0.version,
                    job.0.version_rolling_allowed,
                );
                self.jobs.insert(channel_id, partial_job);
            }
            (None, Some(_), Some(_)) => {
                self.jobs.insert(channel_id, partial_job);
            }
            (None, None, None) => {
                self.jobs.insert(channel_id, partial_job);
            }
            (Some(_), None, None) => {
                self.jobs.insert(channel_id, partial_job);
            }
            (_, Some(_), None) => {
                panic!("impossible state")
            }
            (_, None, Some(_)) => {
                panic!("impossible state")
            }
        };

        let message = OpenExtendedMiningChannelSuccess {
            request_id,
            target,
            channel_id,
            extranonce_size: 16,
            extranonce_prefix: extended.try_into().unwrap(),
        };
        Ok(SendTo::Respond(Mining::OpenExtendedMiningChannelSuccess(
            message,
        )))
    }

    fn handle_update_channel(&mut self, _: UpdateChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        match self.check_target(m.channel_id, m.nonce, m.version, m.ntime, None) {
            Ok(VelideateTargetResult::LessThanBitcoinTarget(_, new_shares_sum, solution)) => {
                // That unwrap means lose a block!!! TODO
                self.solution_sender.try_send(solution).unwrap();
                Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                    SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum,
                    },
                )))
            }
            Ok(VelideateTargetResult::LessThanDownstreamTarget(_, new_shares_sum)) => Ok(
                SendTo::Respond(Mining::SubmitSharesSuccess(SubmitSharesSuccess {
                    channel_id: m.channel_id,
                    last_sequence_number: m.sequence_number,
                    new_submits_accepted_count: 1,
                    new_shares_sum,
                })),
            ),
            Ok(VelideateTargetResult::Invalid(_)) => Ok(SendTo::Respond(
                Mining::SubmitSharesError(SubmitSharesError {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    error_code: "difficulty-too-low".to_string().try_into().unwrap(),
                }),
            )),
            Err(()) => Ok(SendTo::None(None)),
        }
    }

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        match self.check_target(
            m.channel_id,
            m.nonce,
            m.version,
            m.ntime,
            Some(m.extranonce.inner_as_ref()),
        ) {
            Ok(VelideateTargetResult::LessThanBitcoinTarget(_, new_shares_sum, solution)) => {
                // That unwrap means lose a block!!! TODO
                self.solution_sender.try_send(solution).unwrap();
                Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                    SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum,
                    },
                )))
            }
            Ok(VelideateTargetResult::LessThanDownstreamTarget(_, new_shares_sum)) => Ok(
                SendTo::Respond(Mining::SubmitSharesSuccess(SubmitSharesSuccess {
                    channel_id: m.channel_id,
                    last_sequence_number: m.sequence_number,
                    new_submits_accepted_count: 1,
                    new_shares_sum,
                })),
            ),
            Ok(VelideateTargetResult::Invalid(_)) => Ok(SendTo::Respond(
                Mining::SubmitSharesError(SubmitSharesError {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    error_code: "difficulty-too-low".to_string().try_into().unwrap(),
                }),
            )),
            Err(()) => Ok(SendTo::None(None)),
        }
    }

    fn handle_set_custom_mining_job(&mut self, _: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        todo!()
    }
}
