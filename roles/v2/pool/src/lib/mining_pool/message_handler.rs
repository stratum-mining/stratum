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
    utils::Mutex,
};
use std::{convert::TryInto, sync::Arc};

// [h/s] Expected hash rate of the device (or cumulative hashrate on the
// channel if multiple devices are connected downstream) in h/s.
// Depending on server’s target setting policy, this value can be used for
// setting a reasonable target for the channel. Proxy MUST send 0.0f when
// there are no mining devices connected yet.
pub fn hash_rate_to_target(_hs: f32) -> U256<'static> {
    vec![
        0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ]
    .try_into()
    .unwrap()
}

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
        SupportedChannelTypes::Group
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
        let target = hash_rate_to_target(incoming.nominal_hash_rate);
        let extranonce_prefix = self
            .extranonces
            .safe_lock(|e| e.next_standard().unwrap().into_b032())
            .unwrap();
        let message = match (self.downstream_data.header_only, self.id) {
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
                        partial_job.update_job(&job.0, *n_bits, *p_hash, job.1);
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

                OpenStandardMiningChannelSuccess {
                    request_id: request_id.into(),
                    channel_id,
                    target,
                    extranonce_prefix,
                    group_channel_id,
                }
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
                        partial_job.update_job(&job.0, *n_bits, *p_hash, job.1);
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

                OpenStandardMiningChannelSuccess {
                    request_id: request_id.into(),
                    channel_id,
                    group_channel_id: crate::HOM_GROUP_ID,
                    target,
                    extranonce_prefix,
                }
            }
        };
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            Mining::OpenStandardMiningChannelSuccess(message),
        ))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_update_channel(&mut self, _: UpdateChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        match self.check_target(&m) {
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
        _: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(&mut self, _: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        todo!()
    }
}
