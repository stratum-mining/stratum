use super::super::mining_pool::Downstream;
use binary_sv2::{Str0255, B032, U256};
use roles_logic_sv2::{
    channel_management::{
        extended_channel::ExtendedChannelError,
        extended_channel_factory::ExtendedChannelFactoryError,
        standard_channel_factory::StandardChannelFactoryError, ShareValidationError,
        ShareValidationResult,
    },
    errors::Error,
    handlers::mining::{ParseMiningMessagesFromDownstream, SendTo, SupportedChannelTypes},
    mining_sv2::*,
    parsers::Mining,
    template_distribution_sv2::SubmitSolution,
    utils::Mutex,
};
use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};
use tracing::{debug, info};

impl ParseMiningMessagesFromDownstream<()> for Downstream {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }

    fn is_work_selection_enabled(&self) -> bool {
        true
    }

    fn is_downstream_authorized(
        _self_mutex: Arc<Mutex<Self>>,
        _user_identity: &Str0255,
    ) -> Result<bool, Error> {
        Ok(true)
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        info!(
            "Received OpenStandardMiningChannel from: {} with id: {}",
            std::str::from_utf8(incoming.user_identity.as_ref()).unwrap_or("Unknown identity"),
            incoming.get_request_id_as_u32()
        );
        debug!("OpenStandardMiningChannel: {:?}", incoming);
        let request_id = incoming.get_request_id_as_u32();

        // naive approach: create one new group channel for each standard channel
        let group_channel_id = self
            .channel_id_factory
            .next()
            .map_err(|_| Error::FailedToCreateNewChannelId)?;

        self.standard_channel_factory
            .new_group_channel(group_channel_id)
            .map_err(|_| Error::FailedToCreateGroupChannel)?;

        let standard_channel_id = self
            .channel_id_factory
            .next()
            .map_err(|_| Error::FailedToCreateNewChannelId)?;

        match self
            .standard_channel_factory
            .on_open_standard_mining_channel(
                incoming.into_static(),
                group_channel_id,
                standard_channel_id,
            ) {
            Ok(channel_id) => channel_id,
            Err(_) => {
                let open_mining_channel_error = OpenMiningChannelError {
                    request_id: request_id.into(),
                    error_code: Str0255::try_from("".to_string())
                        .expect("Empty string should convert to Str0255"),
                };
                return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                    open_mining_channel_error,
                )));
            }
        };
        let channel = self
            .standard_channel_factory
            .get_standard_channel(standard_channel_id)
            .map_err(|_| Error::FailedToGetStandardChannelFromFactory)?;

        let target: U256 = channel.get_target().into();

        // Convert extranonce_prefix from Vec<u8> to B032 via Extranonce
        let extranonce_prefix_vec = channel.get_extranonce_prefix();
        let extranonce_prefix: B032 = Extranonce::try_from(extranonce_prefix_vec)
            .expect("extranonce_prefix is expected to be valid")
            .into();

        // OpenStandardMiningChannel.Success
        let open_standard_mining_channel_success = OpenStandardMiningChannelSuccess {
            request_id: request_id.into(),
            group_channel_id,
            channel_id: standard_channel_id,
            target,
            extranonce_prefix,
        };

        // active NewMiningJob
        let active_job = channel.get_active_job().ok_or(Error::NoActiveJob)?;

        let messages_res = vec![
            Mining::OpenStandardMiningChannelSuccess(open_standard_mining_channel_success),
            Mining::NewMiningJob(active_job.get_job_message()),
        ];

        let messages = messages_res.into_iter().map(SendTo::Respond).collect();

        Ok(SendTo::Multiple(messages))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        m: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        info!(
            "Received OpenExtendedMiningChannel from: {} with id: {}",
            std::str::from_utf8(m.user_identity.as_ref()).unwrap_or("Unknown identity"),
            m.get_request_id_as_u32()
        );
        debug!("OpenExtendedMiningChannel: {:?}", m);
        let request_id = m.request_id;

        // HOM downstreams are not allowed to open extended channels
        let header_only = self.downstream_data.header_only;
        if header_only {
            return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                OpenMiningChannelError {
                    request_id: request_id.into(),
                    error_code: Str0255::try_from("".to_string())
                        .expect("Empty string should convert to Str0255"),
                },
            )));
        }

        let extended_channel_id = self
            .channel_id_factory
            .next()
            .map_err(|_| Error::FailedToCreateNewChannelId)?;

        match self
            .extended_channel_factory
            .on_open_extended_mining_channel(m.clone().into_static(), extended_channel_id)
        {
            Ok(channel_id) => channel_id,
            Err(_) => {
                let open_mining_channel_error = OpenMiningChannelError {
                    request_id: request_id.into(),
                    error_code: Str0255::try_from("".to_string())
                        .expect("Empty string should convert to Str0255"),
                };
                return Ok(SendTo::Respond(Mining::OpenMiningChannelError(
                    open_mining_channel_error,
                )));
            }
        };
        let channel = self
            .extended_channel_factory
            .get_extended_channel(extended_channel_id)
            .map_err(|_| Error::FailedToGetExtendedChannelFromFactory)?;

        let target: U256 = channel.get_target().into();

        // Convert extranonce_prefix from Vec<u8> to B032 via Extranonce
        let extranonce_prefix_vec = channel.get_extranonce_prefix();
        let extranonce_prefix: B032 = Extranonce::try_from(extranonce_prefix_vec)
            .expect("extranonce_prefix is expected to be valid")
            .into();

        let rollable_extranonce_size: u16 = channel.get_rollable_extranonce_size();

        // OpenExtendeddMiningChannel.Success
        let open_extended_mining_channel_success = OpenExtendedMiningChannelSuccess {
            request_id: request_id.into(),
            channel_id: extended_channel_id,
            target,
            extranonce_prefix,
            extranonce_size: rollable_extranonce_size,
        };

        // active NewExtendedMiningJob
        let active_job = channel.get_active_job().ok_or(Error::NoActiveJob)?;

        let messages_res = vec![
            Mining::OpenExtendedMiningChannelSuccess(open_extended_mining_channel_success),
            Mining::NewExtendedMiningJob(active_job.get_job_message()),
        ];

        let messages = messages_res.into_iter().map(SendTo::Respond).collect();

        Ok(SendTo::Multiple(messages))
    }

    fn handle_update_channel(&mut self, m: UpdateChannel) -> Result<SendTo<()>, Error> {
        info!("Received UpdateChannel message");

        let has_standard_channel = self
            .standard_channel_factory
            .has_standard_channel(m.channel_id)
            .map_err(|_| Error::FailedToCheckIfChannelExists)?;
        let has_extended_channel = self
            .extended_channel_factory
            .has_extended_channel(m.channel_id)
            .map_err(|_| Error::FailedToCheckIfChannelExists)?;

        let set_target = if has_standard_channel {
            let target = self
                .standard_channel_factory
                .update_standard_channel(m.clone().into_static())
                .map_err(|_| Error::FailedToUpdateStandardChannel)?;
            SetTarget {
                channel_id: m.channel_id,
                maximum_target: target.into(),
            }
        } else if has_extended_channel {
            let target = self
                .extended_channel_factory
                .update_extended_channel(m.clone().into_static())
                .map_err(|_| Error::FailedToUpdateExtendedChannel)?;
            SetTarget {
                channel_id: m.channel_id,
                maximum_target: target.into(),
            }
        } else {
            let update_channel_error = UpdateChannelError {
                channel_id: m.channel_id,
                error_code: Str0255::try_from("invalid-channel-id".to_string())
                    .expect("string should convert to Str0255"),
            };
            return Ok(SendTo::Respond(Mining::UpdateChannelError(
                update_channel_error,
            )));
        };

        Ok(SendTo::Respond(Mining::SetTarget(set_target)))
    }

    fn handle_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        info!("Received SubmitSharesStandard");
        debug!("SubmitSharesStandard {:?}", m);

        let channel_id = m.channel_id;

        let has_standard_channel = self
            .standard_channel_factory
            .has_standard_channel(channel_id)
            .map_err(|_| Error::FailedToCheckIfChannelExists)?;

        if !has_standard_channel {
            return Ok(SendTo::Respond(Mining::SubmitSharesError(
                SubmitSharesError {
                    channel_id,
                    sequence_number: m.sequence_number,
                    error_code: Str0255::try_from("invalid-channel-id".to_string())
                        .expect("string should convert to Str0255"),
                },
            )));
        }

        let res = self
            .standard_channel_factory
            .on_submit_shares_standard(m.clone().into_static());
        match res {
            Ok(res) => match res {
                ShareValidationResult::Valid => {
                    return Ok(SendTo::None(None));
                }
                ShareValidationResult::ValidWithAcknowledgement => {
                    let standard_channel = self
                        .standard_channel_factory
                        .get_standard_channel(m.channel_id)
                        .map_err(|_| Error::FailedToGetStandardChannelFromFactory)?;
                    let submit_shares_success = standard_channel.get_shares_acknowledgement();
                    return Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                        submit_shares_success,
                    )));
                }
                ShareValidationResult::BlockFound(template_id, coinbase) => {
                    if template_id.is_some() {
                        let solution = SubmitSolution {
                            template_id: template_id.unwrap(),
                            version: m.version,
                            header_timestamp: m.ntime,
                            header_nonce: m.nonce,
                            coinbase_tx: coinbase.try_into()?,
                        };

                        self.solution_sender
                            .try_send(solution.clone())
                            .expect("Failed to send solution to solution_sender");
                    }
                    let standard_channel = self
                        .standard_channel_factory
                        .get_standard_channel(m.channel_id)
                        .map_err(|_| Error::FailedToGetStandardChannelFromFactory)?;
                    let submit_shares_success = standard_channel.get_shares_acknowledgement();
                    return Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                        submit_shares_success,
                    )));
                }
            },
            Err(e) => match e {
                StandardChannelFactoryError::ShareRejected(e) => {
                    let error_code = match e {
                        ShareValidationError::Invalid => "invalid-share",
                        ShareValidationError::Stale => "stale-share",
                        ShareValidationError::InvalidJobId => "invalid-job-id",
                        ShareValidationError::InvalidChannelId => "invalid-channel-id",
                        ShareValidationError::DoesNotMeetTarget => "difficulty-too-low",
                        ShareValidationError::VersionRollingNotAllowed => {
                            "version-rolling-not-allowed"
                        }
                    };

                    let submit_shares_error = SubmitSharesError {
                        channel_id: m.channel_id,
                        sequence_number: m.sequence_number,
                        error_code: Str0255::try_from(error_code.to_string())
                            .expect("string should convert to Str0255"),
                    };
                    return Ok(SendTo::Respond(Mining::SubmitSharesError(
                        submit_shares_error,
                    )));
                }
                _ => return Err(Error::FailedToValidateShare),
            },
        }
    }

    fn handle_submit_shares_extended(
        &mut self,
        m: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        info!("Received SubmitSharesExtended message");
        debug!("SubmitSharesExtended {:?}", m);

        let channel_id = m.channel_id;

        let has_extended_channel = self
            .extended_channel_factory
            .has_extended_channel(channel_id)
            .map_err(|_| Error::FailedToCheckIfChannelExists)?;

        if !has_extended_channel {
            return Ok(SendTo::Respond(Mining::SubmitSharesError(
                SubmitSharesError {
                    channel_id,
                    sequence_number: m.sequence_number,
                    error_code: Str0255::try_from("invalid-channel-id".to_string())
                        .expect("string should convert to Str0255"),
                },
            )));
        }

        let res = self
            .extended_channel_factory
            .on_submit_shares_extended(m.clone().into_static());

        match res {
            Ok(res) => match res {
                ShareValidationResult::Valid => {
                    return Ok(SendTo::None(None));
                }
                ShareValidationResult::ValidWithAcknowledgement => {
                    let extended_channel = self
                        .extended_channel_factory
                        .get_extended_channel(m.channel_id)
                        .map_err(|_| Error::FailedToGetExtendedChannelFromFactory)?;
                    let submit_shares_success = extended_channel.get_shares_acknowledgement();
                    return Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                        submit_shares_success,
                    )));
                }
                ShareValidationResult::BlockFound(template_id, coinbase) => {
                    if template_id.is_some() {
                        let solution = SubmitSolution {
                            template_id: template_id.unwrap(),
                            version: m.version,
                            header_timestamp: m.ntime,
                            header_nonce: m.nonce,
                            coinbase_tx: coinbase.try_into()?,
                        };

                        self.solution_sender
                            .try_send(solution.clone())
                            .expect("Failed to send solution to solution_sender");
                    }

                    let extended_channel = self
                        .extended_channel_factory
                        .get_extended_channel(m.channel_id)
                        .map_err(|_| Error::FailedToGetExtendedChannelFromFactory)?;
                    let submit_shares_success = extended_channel.get_shares_acknowledgement();
                    return Ok(SendTo::Respond(Mining::SubmitSharesSuccess(
                        submit_shares_success,
                    )));
                }
            },
            Err(e) => match e {
                ExtendedChannelFactoryError::ShareRejected(e) => {
                    let error_code = match e {
                        ShareValidationError::Invalid => "invalid-share",
                        ShareValidationError::Stale => "stale-share",
                        ShareValidationError::InvalidJobId => "invalid-job-id",
                        ShareValidationError::InvalidChannelId => "invalid-channel-id",
                        ShareValidationError::DoesNotMeetTarget => "difficulty-too-low",
                        ShareValidationError::VersionRollingNotAllowed => {
                            "version-rolling-not-allowed"
                        }
                    };

                    let submit_shares_error = SubmitSharesError {
                        channel_id: m.channel_id,
                        sequence_number: m.sequence_number,
                        error_code: Str0255::try_from(error_code.to_string())
                            .expect("string should convert to Str0255"),
                    };
                    return Ok(SendTo::Respond(Mining::SubmitSharesError(
                        submit_shares_error,
                    )));
                }
                _ => return Err(Error::FailedToValidateShare),
            },
        }
    }

    fn handle_set_custom_mining_job(&mut self, m: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        info!(
            "Received SetCustomMiningJob message for channel: {}, with id: {}",
            m.channel_id, m.request_id
        );
        debug!("SetCustomMiningJob: {:?}", m);

        let request_id = m.request_id;

        let channel_id = m.channel_id;

        let has_extended_channel = self
            .extended_channel_factory
            .has_extended_channel(channel_id)
            .map_err(|_| Error::FailedToCheckIfChannelExists)?;

        if !has_extended_channel {
            return Ok(SendTo::Respond(Mining::SetCustomMiningJobError(
                SetCustomMiningJobError {
                    channel_id,
                    request_id,
                    error_code: Str0255::try_from("invalid-channel-id".to_string())
                        .expect("string should convert to Str0255"),
                },
            )));
        }

        // this is a naive pool
        // ideally, it should do some validation on:
        // - m.mining_job_token
        // - m.coinbase_tx_outputs

        let mut extended_channel = self
            .extended_channel_factory
            .get_extended_channel(channel_id)
            .map_err(|_| Error::FailedToGetExtendedChannelFromFactory)?;

        let job_id = match extended_channel.push_custom_job(m.clone().into_static()) {
            Ok(job_id) => job_id,
            Err(e) => match e {
                ExtendedChannelError::CustomJobInvalidPrevHash => {
                    return Ok(SendTo::Respond(Mining::SetCustomMiningJobError(
                        SetCustomMiningJobError {
                            channel_id,
                            request_id,
                            error_code: Str0255::try_from(
                                "invalid-job-param-value-prev-hash".to_string(),
                            )
                            .expect("string should convert to Str0255"),
                        },
                    )));
                }
                ExtendedChannelError::CustomJobInvalidNbits => {
                    return Ok(SendTo::Respond(Mining::SetCustomMiningJobError(
                        SetCustomMiningJobError {
                            channel_id,
                            request_id,
                            error_code: Str0255::try_from(
                                "invalid-job-param-value-nbits".to_string(),
                            )
                            .expect("string should convert to Str0255"),
                        },
                    )));
                }
                ExtendedChannelError::CustomJobInvalidMinNtime => {
                    return Ok(SendTo::Respond(Mining::SetCustomMiningJobError(
                        SetCustomMiningJobError {
                            channel_id,
                            request_id,
                            error_code: Str0255::try_from(
                                "invalid-job-param-value-min-ntime".to_string(),
                            )
                            .expect("string should convert to Str0255"),
                        },
                    )));
                }
                _ => return Err(Error::FailedToSetCustomMiningJob),
            },
        };

        let set_custom_mining_job_success = SetCustomMiningJobSuccess {
            channel_id,
            request_id,
            job_id,
        };

        Ok(SendTo::Respond(Mining::SetCustomMiningJobSuccess(
            set_custom_mining_job_success,
        )))
    }
}
