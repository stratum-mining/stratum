use crate::lib::mining_pool::Downstream;
use roles_logic_sv2::{
    common_properties::IsDownstream,
    errors::Error,
    handlers::mining::{ParseDownstreamMiningMessages, SendTo, SupportedChannelTypes},
    mining_sv2::*,
    parsers::Mining,
    routing_logic::{MiningRouter, NoRouting},
    selectors::NullDownstreamMiningSelector,
    template_distribution_sv2::SubmitSolution,
    utils::Mutex,
};
use std::{convert::TryInto, sync::Arc};
use tracing::{debug, error, info};

impl ParseDownstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Downstream {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled(&self) -> bool {
        true
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
        _m: Option<Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        debug!(
            "Handling open standard mining channel request_id: {} for hash_rate: {}",
            incoming.request_id.as_u32(),
            incoming.nominal_hash_rate
        );
        let header_only = self.downstream_data.header_only;
        let reposnses = self
            .channel_factory
            .safe_lock(|factory| {
                match factory.add_standard_channel(
                    incoming.request_id.as_u32(),
                    incoming.nominal_hash_rate,
                    header_only,
                    self.id,
                ) {
                    Ok(msgs) => {
                        let mut res = vec![];
                        for msg in msgs {
                            res.push(msg.into_static());
                        }
                        Ok(res)
                    }
                    Err(e) => Err(e),
                }
            })
            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))??;
        let mut result = vec![];
        for response in reposnses {
            result.push(SendTo::Respond(response.into_static()))
        }
        Ok(SendTo::Multiple(result))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        let request_id = m.request_id;
        let hash_rate = m.nominal_hash_rate;
        let min_extranonce_size = m.min_extranonce_size;
        let messages_res = self
            .channel_factory
            .safe_lock(|s| s.new_extended_channel(request_id, hash_rate, min_extranonce_size))
            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?;
        match messages_res {
            Some(messages) => {
                let messages = messages.into_iter().map(SendTo::Respond).collect();
                Ok(SendTo::Multiple(messages))
            }
            None => Err(roles_logic_sv2::Error::ChannelIsNeitherExtendedNeitherInAPool),
        }
    }

    fn handle_update_channel(&mut self, _: UpdateChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        let res = self
            .channel_factory
            .safe_lock(|cf| cf.on_submit_shares_standard(m.clone()))
            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?;
        match res {
            Ok(res) => match res  {
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::SendErrorDownstream(m) => {
                    Ok(SendTo::Respond(Mining::SubmitSharesError(m)))
                }
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::SendSubmitShareUpstream(_) => unreachable!(),
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::RelaySubmitShareUpstream => unreachable!(),
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::ShareMeetBitcoinTarget((share,t_id,coinbase)) => {
                    info!("Found share that meet bitcoin target");
                    let solution = SubmitSolution {
                        template_id: t_id,
                        version: share.get_version(),
                        header_timestamp: share.get_n_time(),
                        header_nonce: share.get_nonce(),
                        coinbase_tx: coinbase.try_into()?,
                    };
                    // TODO we can block everything with the below (looks like this will infinite loop??)
                    while self.solution_sender.try_send(solution.clone()).is_err() {};
                    let success = SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum: 0,
                    };

                    Ok(SendTo::Respond(Mining::SubmitSharesSuccess(success)))

                },
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::ShareMeetDownstreamTarget => {
                 let success = SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum: 0,
                    };
                    Ok(SendTo::Respond(Mining::SubmitSharesSuccess(success)))
                },
            },
            Err(_) => todo!(),
        }
    }

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        debug!("Handling submit share extended {:#?}", m);
        let res = self
            .channel_factory
            .safe_lock(|cf| cf.on_submit_shares_extended(m.clone()))
            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?;
        match res {
            Ok(res) => match res  {
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::SendErrorDownstream(m) => {
                    Ok(SendTo::Respond(Mining::SubmitSharesError(m)))
                }
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::SendSubmitShareUpstream(_) => unreachable!(),
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::RelaySubmitShareUpstream => unreachable!(),
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::ShareMeetBitcoinTarget((share,t_id,coinbase)) => {
                    info!("Found share that meet bitcoin target");
                    let solution = SubmitSolution {
                        template_id: t_id,
                        version: share.get_version(),
                        header_timestamp: share.get_n_time(),
                        header_nonce: share.get_nonce(),
                        coinbase_tx: coinbase.try_into()?,
                    };
                    // TODO we can block everything with the below (looks like this will infinite loop??)
                    while self.solution_sender.try_send(solution.clone()).is_err() {};
                    let success = SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum: 0,
                    };

                    Ok(SendTo::Respond(Mining::SubmitSharesSuccess(success)))

                },
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::ShareMeetDownstreamTarget => {
                 let success = SubmitSharesSuccess {
                        channel_id: m.channel_id,
                        last_sequence_number: m.sequence_number,
                        new_submits_accepted_count: 1,
                        new_shares_sum: 0,
                    };
                    Ok(SendTo::Respond(Mining::SubmitSharesSuccess(success)))
                },
            },
            Err(e) => {
                error!("{:?}",e);
                todo!();
            }
        }
    }

    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        // TODO
        let m = SetCustomMiningJobSuccess {
            channel_id: m.channel_id,
            request_id: m.request_id,
            // TODO save it somewhere so we can match shares (channel factory??)
            job_id: 0,
        };
        Ok(SendTo::Respond(Mining::SetCustomMiningJobSuccess(m)))
    }

    fn handle_message_mining(
        self_mutex: Arc<Mutex<Self>>,
        message_type: u8,
        payload: &mut [u8],
        routing_logic: roles_logic_sv2::routing_logic::MiningRoutingLogic<
            Self,
            (),
            NullDownstreamMiningSelector,
            NoRouting,
        >,
    ) -> Result<SendTo<()>, Error>
    where
        Self: roles_logic_sv2::common_properties::IsMiningDownstream + Sized,
    {
        let (channel_type, is_work_selection_enabled, downstream_mining_data) = self_mutex
            .safe_lock(|self_| {
                (
                    self_.get_channel_type(),
                    self_.is_work_selection_enabled(),
                    self_.get_downstream_mining_data(),
                )
            })
            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?;
        match (message_type, payload).try_into() {
            Ok(Mining::OpenStandardMiningChannel(mut m)) => {
                debug!("Received OpenStandardMiningChannel message");
                let upstream = match routing_logic {
                    roles_logic_sv2::routing_logic::MiningRoutingLogic::None => None,
                    roles_logic_sv2::routing_logic::MiningRoutingLogic::Proxy(r_logic) => {
                        debug!("Routing logic is Proxy");
                        let up = r_logic
                            .safe_lock(|r_logic| {
                                r_logic.on_open_standard_channel(
                                    self_mutex.clone(),
                                    &mut m,
                                    &downstream_mining_data,
                                )
                            })
                            .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?;
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    roles_logic_sv2::routing_logic::MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Extended => {
                    debug!("Received OpenExtendedMiningChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received OpenExtendedMiningChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received UpdateChannel->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => {
                    debug!("Received UpdateChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Group => {
                    debug!("Received UpdateChannel->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received UpdateChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received SubmitSharesStandard->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage(message_type)),
                SupportedChannelTypes::Group => {
                    debug!("Received SubmitSharesStandard->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received SubmitSharesStandard->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => {
                debug!("Received SubmitSharesExtended message");
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage(message_type)),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                }
            }
            Ok(Mining::SetCustomMiningJob(m)) => {
                debug!("Received SetCustomMiningJob message");
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::Group, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .map_err(|e| roles_logic_sv2::Error::PoisonLock(e.to_string()))?,
                    _ => Err(Error::UnexpectedMessage(message_type)),
                }
            }
            Ok(_) => todo!(),
            Err(e) => Err(e),
        }
    }
}
