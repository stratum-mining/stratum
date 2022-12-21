use crate::lib::mining_pool::Downstream;
use roles_logic_sv2::{
    channel_logic::channel_factory::OpenStandardChannleRequester,
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
use tracing::{debug, info};

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
        debug!(
            "Handling open standard mining channel request_id: {} for hash_rate: {}",
            incoming.request_id.as_u32(),
            incoming.nominal_hash_rate
        );
        let requester = match self.downstream_data.header_only {
            true => OpenStandardChannleRequester::HomRequester,
            false => OpenStandardChannleRequester::NonHomRequester { group_id: self.id },
        };
        let reposnses = self
            .channel_factory
            .safe_lock(|factory| {
                match factory.add_standard_channel(
                    incoming.request_id.as_u32(),
                    incoming.nominal_hash_rate,
                    requester,
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
            .unwrap()?;
        let mut result = vec![];
        for response in reposnses {
            result.push(SendTo::Respond(response.into_static()))
        }
        Ok(SendTo::Multiple(result))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        // TODO add support for extended channel
        todo!()
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
            .unwrap();
        match res {
            Ok(res) => match res  {
                roles_logic_sv2::channel_logic::channel_factory::OnNewShare::SendErrorDowsntream(m) => {
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
                        coinbase_tx: coinbase.try_into().unwrap(),
                    };
                    // TODO we can block everything with the below
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
        _m: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(&mut self, _: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        todo!()
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
            .unwrap();
        // Is fine to unwrap on safe_lock
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
                            .unwrap();
                        Some(up?)
                    }
                    // Variant just used for phantom data is ok to panic
                    roles_logic_sv2::routing_logic::MiningRoutingLogic::_P(_) => panic!(),
                };
                match channel_type {
                    SupportedChannelTypes::Standard => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .unwrap(),
                    SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage),
                    SupportedChannelTypes::Group => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .unwrap(),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_open_standard_mining_channel(m, upstream))
                        .unwrap(),
                }
            }
            Ok(Mining::OpenExtendedMiningChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage),
                SupportedChannelTypes::Extended => {
                    debug!("Received OpenExtendedMiningChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .unwrap()
                }
                SupportedChannelTypes::Group => Err(Error::UnexpectedMessage),
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received OpenExtendedMiningChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_open_extended_mining_channel(m))
                        .unwrap()
                }
            },
            Ok(Mining::UpdateChannel(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received UpdateChannel->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .unwrap()
                }
                SupportedChannelTypes::Extended => {
                    debug!("Received UpdateChannel->Extended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .unwrap()
                }
                SupportedChannelTypes::Group => {
                    debug!("Received UpdateChannel->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .unwrap()
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received UpdateChannel->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_update_channel(m))
                        .unwrap()
                }
            },
            Ok(Mining::SubmitSharesStandard(m)) => match channel_type {
                SupportedChannelTypes::Standard => {
                    debug!("Received SubmitSharesStandard->Standard message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .unwrap()
                }
                SupportedChannelTypes::Extended => Err(Error::UnexpectedMessage),
                SupportedChannelTypes::Group => {
                    debug!("Received SubmitSharesStandard->Group message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .unwrap()
                }
                SupportedChannelTypes::GroupAndExtended => {
                    debug!("Received SubmitSharesStandard->GroupAndExtended message");
                    self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_standard(m))
                        .unwrap()
                }
            },
            Ok(Mining::SubmitSharesExtended(m)) => {
                debug!("Received SubmitSharesExtended message");
                match channel_type {
                    SupportedChannelTypes::Standard => Err(Error::UnexpectedMessage),
                    SupportedChannelTypes::Extended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .unwrap(),
                    SupportedChannelTypes::Group => Err(Error::UnexpectedMessage),
                    SupportedChannelTypes::GroupAndExtended => self_mutex
                        .safe_lock(|self_| self_.handle_submit_shares_extended(m))
                        .unwrap(),
                }
            }
            Ok(Mining::SetCustomMiningJob(m)) => {
                debug!("Received SetCustomMiningJob message");
                match (channel_type, is_work_selection_enabled) {
                    (SupportedChannelTypes::Extended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .unwrap(),
                    (SupportedChannelTypes::Group, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .unwrap(),
                    (SupportedChannelTypes::GroupAndExtended, true) => self_mutex
                        .safe_lock(|self_| self_.handle_set_custom_mining_job(m))
                        .unwrap(),
                    _ => Err(Error::UnexpectedMessage),
                }
            }
            Ok(_) => Err(Error::UnexpectedMessage),
            Err(e) => Err(e),
        }
    }
}
