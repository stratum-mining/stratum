use stratum_common::roles_logic_sv2::{
    self,
    bitcoin::Amount,
    channels_sv2::{
        client,
        outputs::deserialize_outputs,
        server::{
            error::{ExtendedChannelError, StandardChannelError},
            extended::ExtendedChannel,
            group::GroupChannel,
            jobs::job_store::DefaultJobStore,
            share_accounting::{ShareValidationError, ShareValidationResult},
            standard::StandardChannel,
        },
    },
    codec_sv2::binary_sv2::Str0255,
    handlers_sv2::{HandleMiningMessagesFromClientAsync, SupportedChannelTypes},
    job_declaration_sv2::PushSolution,
    mining_sv2::*,
    parsers_sv2::{AnyMessage, JobDeclaration, Mining, TemplateDistribution},
    template_distribution_sv2::SubmitSolution,
    Vardiff, VardiffState,
};
use tracing::{debug, error, info, warn};

use crate::{
    channel_manager::{ChannelManager, ChannelManagerChannel},
    error::JDCError,
    jd_mode::{get_jd_mode, JdMode},
    utils::StdFrame,
};

/// `RouteMessageTo` is an abstraction used to route protocol messages
/// to the appropriate subsystem connected to the JDC.
///
/// Instead of manually handling routing logic for each message type,
/// this enum provides a unified interface. Each variant represents
/// a possible destination:
///
/// - [`RouteMessageTo::Upstream`] → For messages intended for the upstream.
/// - [`RouteMessageTo::JobDeclarator`] → For job declaration messages sent to the JDS.
/// - [`RouteMessageTo::TemplateProvider`] → For template distribution messages sent to the template
///   provider.
/// - [`RouteMessageTo::Downstream`] → For messages destined to a specific downstream client,
///   identified by its `u32` downstream ID.
#[derive(Clone)]
pub enum RouteMessageTo<'a> {
    /// Route to the upstream (mining) channel.
    Upstream(Mining<'a>),
    /// Route to the job declarator subsystem.
    JobDeclarator(JobDeclaration<'a>),
    /// Route to the template provider subsystem.
    TemplateProvider(TemplateDistribution<'a>),
    /// Route to a specific downstream client by ID, along with its mining message.
    Downstream((u32, Mining<'a>)),
}

impl<'a> From<Mining<'a>> for RouteMessageTo<'a> {
    fn from(value: Mining<'a>) -> Self {
        Self::Upstream(value)
    }
}

impl<'a> From<JobDeclaration<'a>> for RouteMessageTo<'a> {
    fn from(value: JobDeclaration<'a>) -> Self {
        Self::JobDeclarator(value)
    }
}

impl<'a> From<TemplateDistribution<'a>> for RouteMessageTo<'a> {
    fn from(value: TemplateDistribution<'a>) -> Self {
        Self::TemplateProvider(value)
    }
}

impl<'a> From<(u32, Mining<'a>)> for RouteMessageTo<'a> {
    fn from(value: (u32, Mining<'a>)) -> Self {
        Self::Downstream(value)
    }
}

impl RouteMessageTo<'_> {
    /// Forwards the message to its corresponding destination channel.
    ///
    /// The routing is handled as follows:
    /// - [`RouteMessageTo::Downstream`] → Sends the mining message to the specified downstream
    ///   client.
    /// - [`RouteMessageTo::Upstream`] → Sends the mining message upstream, unless in
    ///   [`JdMode::SoloMining`].
    /// - [`RouteMessageTo::JobDeclarator`] → Sends the job declaration message to the JDS.
    /// - [`RouteMessageTo::TemplateProvider`] → Sends the template distribution message to the
    ///   template provider.
    ///
    /// Messages are automatically converted into the appropriate
    /// [`AnyMessage`] variant and wrapped into a [`StdFrame`].
    pub async fn forward(self, channel_manager_channel: &ChannelManagerChannel) {
        match self {
            RouteMessageTo::Downstream((downstream_id, message)) => {
                _ = channel_manager_channel
                    .downstream_sender
                    .send((downstream_id, AnyMessage::Mining(message).into_static()));
            }
            RouteMessageTo::Upstream(message) => {
                if get_jd_mode() != JdMode::SoloMining {
                    let message = AnyMessage::Mining(message).into_static();
                    let frame: StdFrame = message.try_into().unwrap();
                    _ = channel_manager_channel.upstream_sender.send(frame).await;
                }
            }
            RouteMessageTo::JobDeclarator(message) => {
                let message = AnyMessage::JobDeclaration(message).into_static();
                let frame: StdFrame = message.try_into().unwrap();
                _ = channel_manager_channel.jd_sender.send(frame).await;
            }
            RouteMessageTo::TemplateProvider(message) => {
                let message = AnyMessage::TemplateDistribution(message).into_static();
                let frame: StdFrame = message.try_into().unwrap();
                _ = channel_manager_channel.tp_sender.send(frame).await;
            }
        }
    }
}

impl HandleMiningMessagesFromClientAsync for ChannelManager {
    type Error = JDCError;

    fn get_channel_type_for_client(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::GroupAndExtended
    }
    fn is_work_selection_enabled_for_client(&self) -> bool {
        false
    }
    fn is_client_authorized(&self, _user_identity: &Str0255) -> Result<bool, Self::Error> {
        Ok(true)
    }

    // Handles a `CloseChannel` message:
    // - Look up the downstream associated with the given `channel_id`.
    // - If found, remove the channel from its `extended_channels` and `standard_channels`.
    // - If not found, return an appropriate error.
    async fn handle_close_channel(&mut self, msg: CloseChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        self.channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let Some(downstream_id) = channel_manager_data
                    .channel_id_to_downstream_id
                    .get(&msg.channel_id)
                else {
                    error!(
                        "No downstream_id related to channel_id: {:?}, found",
                        msg.channel_id
                    );
                    return Err(JDCError::DownstreamNotFoundWithChannelId(msg.channel_id));
                };
                let Some(downstream) = channel_manager_data.downstream.get(downstream_id) else {
                    error!(
                        "No downstream with channel_id: {:?} and downstream_id: {:?}, found",
                        msg.channel_id, downstream_id
                    );
                    return Err(JDCError::DownstreamNotFound(*downstream_id));
                };
                downstream.downstream_data.super_safe_lock(|data| {
                    data.extended_channels.remove(&msg.channel_id);
                    data.standard_channels.remove(&msg.channel_id);
                });
                Ok(())
            })
    }

    // Handles an `OpenStandardMiningChannel` message from a downstream.
    //
    // Steps:
    // 1. Parse the `downstream_id` from the `user_identity`.
    // 2. Create a new `StandardChannel` for the downstream.
    // 3. Ensure a valid `GroupChannel` exists (create one if needed).
    // 4. Apply the latest future template and prevhash to both group and standard channels.
    // 5. Send the following messages back to the downstream:
    //    - `OpenStandardMiningChannelSuccess`
    //    - `NewMiningJob`
    //    - `SetNewPrevHash`
    // 6. Update the downstream state, including:
    //    - Channel manager mappings
    //    - Standard and group channel registrations
    //    - Vardiff state
    //
    // Returns an error if any step fails, such as missing templates, invalid identity,
    // or failure to apply updates to channels.
    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let request_id = msg.get_request_id_as_u32();
        let user_string = msg.user_identity.as_utf8_or_hex();

        let coinbase_outputs = self
            .channel_manager_data
            .super_safe_lock(|data| data.coinbase_outputs.clone());

        let mut coinbase_outputs = deserialize_outputs(coinbase_outputs)
            .map_err(|_| JDCError::ChannelManagerHasBadCoinbaseOutputs)?;

        let (user_identity, downstream_id) = match user_string.rsplit_once('#') {
            Some((user_identity, id)) => match id.parse::<u32>() {
                Ok(id) => (user_identity, id),
                Err(e) => {
                    warn!(
                        ?e,
                        user_string, "Failed to parse downstream_id from user_identity"
                    );
                    return Err(JDCError::ParseInt(e));
                }
            },
            None => {
                warn!(user_string, "User identity missing downstream_id");
                return Err(JDCError::DownstreamIdNotFound);
            }
        };

        info!(downstream_id, "Received: {}", msg);

        let build_error = |code: &str| {
            Mining::OpenMiningChannelError(OpenMiningChannelError {
                request_id,
                error_code: code.to_string().try_into().expect("valid error code"),
            })
        };

        let messages: Vec<RouteMessageTo> =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {
                    let Some(last_future_template) =
                        channel_manager_data.last_future_template.clone()
                    else {
                        error!("Missing last_future_template, cannot open channel");
                        return Err(JDCError::FutureTemplateNotPresent);
                    };

                    let Some(last_new_prev_hash) = channel_manager_data.last_new_prev_hash.clone()
                    else {
                        error!("Missing last_new_prev_hash, cannot open channel");
                        return Err(JDCError::LastNewPrevhashNotFound);
                    };

                    let Some(downstream) = channel_manager_data.downstream.get(&downstream_id)
                    else {
                        error!(downstream_id, "Downstream not registered");
                        return Err(JDCError::DownstreamNotFound(downstream_id));
                    };

                    coinbase_outputs[0].value =
                        Amount::from_sat(last_future_template.coinbase_tx_value_remaining);

                    downstream.downstream_data.super_safe_lock(|data| {
                        let mut messages: Vec<RouteMessageTo> = vec![];

                        if !data.require_std_job && data.group_channels.is_none() {
                            let group_channel_id = channel_manager_data.channel_id_factory.next();
                            let job_store = DefaultJobStore::new();
                            let mut group_channel = GroupChannel::new_for_job_declaration_client(
                                group_channel_id,
                                job_store,
                                channel_manager_data.pool_tag_string.clone(),
                                self.miner_tag_string.clone(),
                            );

                            if let Err(e) = group_channel.on_new_template(
                                last_future_template.clone(),
                                coinbase_outputs.clone(),
                            ) {
                                error!(?e, "Failed to apply template to group channel");
                                return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessNewTemplateGroupChannel(e)));
                            }

                            if let Err(e) =
                                group_channel.on_set_new_prev_hash(last_new_prev_hash.clone())
                            {
                                error!(?e, "Failed to apply prevhash to group channel");
                                return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessSetNewPrevHashGroupChannel(e)));
                            };

                            data.group_channels = Some(group_channel);
                        }

                        let nominal_hash_rate = msg.nominal_hash_rate;
                        let requested_max_target = msg.max_target.into_static();

                        let group_channel_id = data
                            .group_channels
                            .as_ref()
                            .map(|gc| gc.get_group_channel_id())
                            .unwrap_or(0);
                        let standard_channel_id = channel_manager_data.channel_id_factory.next();

                        let extranonce_prefix = match channel_manager_data
                            .extranonce_prefix_factory_standard
                            .next_prefix_standard()
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(?e, "Failed to get extranonce prefix");
                                return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::ExtranoncePrefixFactoryError(e)));
                            }
                        };

                        let job_store = DefaultJobStore::new();
                        let mut standard_channel =
                            match StandardChannel::new_for_job_declaration_client(
                                standard_channel_id,
                                user_identity.to_string(),
                                extranonce_prefix.to_vec(),
                                requested_max_target.into(),
                                nominal_hash_rate,
                                self.share_batch_size,
                                self.shares_per_minute,
                                job_store,
                                channel_manager_data.pool_tag_string.clone(),
                                self.miner_tag_string.clone(),
                            ) {
                                Ok(channel) => channel,
                                Err(e) => {
                                    error!(?e, "Failed to create standard channel");
                                    return match e {
                                        StandardChannelError::InvalidNominalHashrate => {
                                            Ok(vec![(downstream_id, build_error("invalid-nominal-hashrate")).into()])
                                        }
                                        StandardChannelError::RequestedMaxTargetOutOfRange => {
                                            Ok(vec![(downstream_id, build_error("max-target-out-of-range")).into()])
                                        }
                                        other => Err(
                                            JDCError::RolesSv2Logic(
                                                roles_logic_sv2::Error::FailedToCreateStandardChannel(other)
                                            )
                                        ),
                                    }
                                }
                            };

                        let open_standard_mining_channel_success =
                            OpenStandardMiningChannelSuccess {
                                request_id: msg.request_id.clone(),
                                channel_id: standard_channel_id,
                                target: standard_channel.get_target().clone().into(),
                                extranonce_prefix: standard_channel
                                    .get_extranonce_prefix()
                                    .clone()
                                    .try_into()
                                    .expect("extranonce_prefix must be valid"),
                                group_channel_id,
                            }
                            .into_static();

                        messages.push(
                            (
                                downstream_id,
                                Mining::OpenStandardMiningChannelSuccess(
                                    open_standard_mining_channel_success,
                                ),
                            )
                                .into(),
                        );

                        if let Err(e) = standard_channel
                            .on_new_template(last_future_template.clone(), coinbase_outputs.clone())
                        {
                            error!(?e, "Failed to apply template to standard channel");
                            return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessNewTemplateStandardChannel(e)));
                        }

                        let future_standard_job_id = standard_channel
                            .get_future_template_to_job_id()
                            .get(&last_future_template.template_id)
                            .cloned()
                            .expect("future job id must exist");

                        let future_standard_job = standard_channel
                            .get_future_jobs()
                            .get(&future_standard_job_id)
                            .expect("future job must exist");

                        let future_standard_job_message =
                            future_standard_job.get_job_message().clone().into_static();

                        messages.push(
                            (
                                downstream_id,
                                Mining::NewMiningJob(future_standard_job_message),
                            )
                                .into(),
                        );

                        let prev_hash = last_new_prev_hash.prev_hash.clone();
                        let header_timestamp = last_new_prev_hash.header_timestamp;
                        let n_bits = last_new_prev_hash.n_bits;
                        let set_new_prev_hash_mining = SetNewPrevHash {
                            channel_id: standard_channel_id,
                            job_id: future_standard_job_id,
                            prev_hash,
                            min_ntime: header_timestamp,
                            nbits: n_bits,
                        };

                        if let Err(e) =
                            standard_channel.on_set_new_prev_hash(last_new_prev_hash.clone())
                        {
                            error!(?e, "Failed to apply prevhash to standard channel");
                            return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessSetNewPrevHashStandardChannel(e)));
                        }
                        messages.push(
                            (
                                downstream_id,
                                Mining::SetNewPrevHash(set_new_prev_hash_mining),
                            )
                                .into(),
                        );

                        let vardiff = VardiffState::new().expect("Vardiff state should instantiate.");

                        channel_manager_data.vardiff.insert((standard_channel_id, downstream_id),vardiff);
                        data.standard_channels.insert(standard_channel_id, standard_channel);
                        channel_manager_data
                            .channel_id_to_downstream_id
                            .insert(standard_channel_id, downstream_id);

                        channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((standard_channel_id, future_standard_job_id), last_future_template.template_id);
                        if let Some(group_channel) = data.group_channels.as_mut() {
                            group_channel.add_standard_channel_id(standard_channel_id);
                        }

                        Ok(messages)
                    })
                })?;

        for messages in messages {
            messages.forward(&self.channel_manager_channel).await;
        }
        Ok(())
    }

    // Handles an `OpenExtendedMiningChannel` request from a downstream.
    //
    // Workflow:
    // 1. Extract the `downstream_id` from `user_identity`.
    // 2. Create a new `ExtendedChannel` with the requested parameters.
    // 3. Send back to the downstream:
    //    - `OpenExtendedMiningChannelSuccess`
    //    - `NewExtendedMiningJob` (based on the latest future template)
    //    - `SetNewPrevHash` (to immediately activate the job)
    // 4. Update internal state, including:
    //    - Extended channel registry
    //    - Downstream/channel mappings
    //    - Vardiff state
    //
    // Returns an error if the downstream is missing, template/prevhash are unavailable,
    // or if extended channel creation fails.
    async fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel<'_>,
    ) -> Result<(), Self::Error> {
        let user_string = msg.user_identity.as_utf8_or_hex();
        let (user_identity, downstream_id) = match user_string.rsplit_once('#') {
            Some((user_identity, id)) => match id.parse::<u32>() {
                Ok(v) => (user_identity, v),
                Err(e) => {
                    warn!(?e, user_string, "Invalid downstream_id in user_identity");
                    return Err(JDCError::ParseInt(e));
                }
            },
            None => {
                warn!(user_string, "Missing downstream_id in user_identity");
                return Err(JDCError::DownstreamIdNotFound);
            }
        };

        info!(downstream_id, "Received: {}", msg);
        let request_id = msg.get_request_id_as_u32();

        let nominal_hash_rate = msg.nominal_hash_rate;
        let requested_max_target = msg.max_target.into_static();
        let requested_min_rollable_extranonce_size = msg.min_extranonce_size;

        let build_error = |code: &str| {
            Mining::OpenMiningChannelError(OpenMiningChannelError {
                request_id,
                error_code: code.to_string().try_into().expect("valid error code"),
            })
        };

        let messages =
            self.channel_manager_data
                .super_safe_lock(|channel_manager_data| {

                    let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id) else {
                        error!(downstream_id, "Downstream not found");
                        return Err(JDCError::DownstreamNotFound(downstream_id));
                    };

                   downstream.downstream_data.super_safe_lock(|data| {

                        let mut messages: Vec<RouteMessageTo> = vec![];
                        let extended_channel_id = channel_manager_data.channel_id_factory.next();

                        let extranonce_prefix = match channel_manager_data.extranonce_prefix_factory_extended
                            .next_prefix_extended(requested_min_rollable_extranonce_size.into())
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(?e, "Extranonce prefix error");
                                return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::ExtranoncePrefixFactoryError(e)));
                            }
                        };

                        let Some(last_future_template) = channel_manager_data.last_future_template.clone() else {
                            error!("No template to share");
                            return Err(JDCError::FutureTemplateNotPresent);
                        };

                        let Some(last_new_prev_hash) = channel_manager_data.last_new_prev_hash.clone() else {
                            error!("No prevhash in system");
                            return Err(JDCError::LastNewPrevhashNotFound);
                        };

                        let job_store = DefaultJobStore::new();

                        let mut extended_channel = match ExtendedChannel::new_for_job_declaration_client(
                            extended_channel_id,
                            user_identity.to_string(),
                            extranonce_prefix.into(),
                            requested_max_target.into(),
                            nominal_hash_rate,
                            true,
                            requested_min_rollable_extranonce_size,
                            self.share_batch_size,
                            self.shares_per_minute,
                            job_store,
                            channel_manager_data.pool_tag_string.clone(),
                            self.miner_tag_string.clone(),
                        ) {
                            Ok(c) => c,
                            Err(e) => {
                                error!(?e, "Failed to create ExtendedChannel");
                                return match e {
                                    ExtendedChannelError::InvalidNominalHashrate => {
                                        Ok(vec![(downstream_id, build_error("invalid-nominal-hashrate")).into()])
                                    }
                                    ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                        Ok(vec![(downstream_id, build_error("max-target-out-of-range")).into()])
                                    }
                                    ExtendedChannelError::RequestedMinExtranonceSizeTooLarge  => {
                                        Ok(vec![(downstream_id, build_error("min-extranonce-size-too-large")).into()])
                                    }
                                    other => Err(
                                        JDCError::RolesSv2Logic(
                                            roles_logic_sv2::Error::FailedToCreateExtendedChannel(other)
                                        )
                                    ),
                                }
                            }
                        };

                        let open_extended_mining_channel_success =
                            OpenExtendedMiningChannelSuccess {
                                request_id,
                                channel_id: extended_channel_id,
                                target: extended_channel.get_target().clone().into(),
                                extranonce_prefix: extended_channel
                                    .get_extranonce_prefix()
                                    .clone()
                                    .try_into()
                                    .expect("valid extranonce prefix"),
                                extranonce_size: extended_channel.get_rollable_extranonce_size(),
                            }
                            .into_static();

                        messages.push((
                            downstream_id,
                            Mining::OpenExtendedMiningChannelSuccess(
                                open_extended_mining_channel_success,
                            ),
                        ).into());

                        let mut coinbase_outputs = match deserialize_outputs(channel_manager_data.coinbase_outputs.clone()) {
                            Ok(outputs) => outputs,
                            Err(_) => return Err(JDCError::ChannelManagerHasBadCoinbaseOutputs),
                        };
                        coinbase_outputs[0].value =
                            Amount::from_sat(last_future_template.coinbase_tx_value_remaining);


                        // create a future extended job based on the last future template
                        if let Err(e) =
                            extended_channel.on_new_template(last_future_template.clone(), coinbase_outputs)
                        {
                            error!(?e, "Failed to apply template to extended channel");
                            return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessNewTemplateExtendedChannel(e)));
                        }

                        let future_extended_job_id = extended_channel
                            .get_future_template_to_job_id()
                            .get(&last_future_template.template_id)
                            .cloned()
                            .expect("future job id must exist");
                        let future_extended_job = extended_channel
                            .get_future_jobs()
                            .get(&future_extended_job_id)
                            .expect("future job must exist");

                        let future_extended_job_message =
                            future_extended_job.get_job_message().clone().into_static();

                        // send this future job as new job message
                        // to be immediately activated with the subsequent SetNewPrevHash message
                        messages.push((
                            downstream_id,
                            Mining::NewExtendedMiningJob(
                                future_extended_job_message,
                            ),
                        ).into());

                        // SetNewPrevHash message activates the future job
                        let prev_hash = last_new_prev_hash.prev_hash.clone();
                        let header_timestamp = last_new_prev_hash.header_timestamp;
                        let n_bits = last_new_prev_hash.n_bits;
                        let set_new_prev_hash_mining = SetNewPrevHash {
                            channel_id: extended_channel_id,
                            job_id: future_extended_job_id,
                            prev_hash,
                            min_ntime: header_timestamp,
                            nbits: n_bits,
                        };
                        if let Err(e) = extended_channel.on_set_new_prev_hash(last_new_prev_hash) {
                            error!(?e, "Failed to set prevhash on extended channel");
                            return Err(JDCError::RolesSv2Logic(roles_logic_sv2::Error::FailedToProcessSetNewPrevHashExtendedChannel(e)));
                        }
                        messages.push((
                            downstream_id,
                            Mining::SetNewPrevHash(set_new_prev_hash_mining),
                        ).into());

                        let vardiff = VardiffState::new().expect("Vardiff should instantiate.");
                        data.extended_channels.insert(extended_channel_id, extended_channel);

                        channel_manager_data.downstream_channel_id_and_job_id_to_template_id.insert((extended_channel_id, future_extended_job_id), last_future_template.template_id);
                        channel_manager_data
                            .channel_id_to_downstream_id
                            .insert(extended_channel_id, downstream_id);
                        channel_manager_data.vardiff.insert((extended_channel_id, downstream_id), vardiff);

                        Ok(messages)
                    })
                })?;

        for messages in messages {
            messages.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    // Handles an `UpdateChannel` message from a downstream.
    //
    // Workflow:
    // 1. Update the target for the corresponding downstream channel (standard or extended).
    //    - On success, reply with a `SetTarget`.
    //    - On failure, return an `UpdateChannelError`.
    // 2. Recompute aggregate downstream state:
    //    - Sum all downstream nominal hashrates.
    //    - Determine the minimum target across all downstream channels.
    // 3. Propagate the update upstream by sending an `UpdateChannel` with the aggregated hashrate
    //    and minimum target.
    //
    // Returns an error if the downstream channel is missing or update
    // validation fails.
    async fn handle_update_channel(&mut self, msg: UpdateChannel<'_>) -> Result<(), Self::Error> {
        info!("Received: {}", msg);
        let channel_id = msg.channel_id;
        let new_nominal_hash_rate = msg.nominal_hash_rate;
        let requested_maximum_target = msg.maximum_target.into_static();

        let messages = self
            .channel_manager_data
            .super_safe_lock(|channel_manager_data| {
                let mut messages: Vec<RouteMessageTo> = vec![];

                let downstream_id = match channel_manager_data
                    .channel_id_to_downstream_id
                    .get(&channel_id)
                {
                    Some(id) => *id,
                    None => {
                        error!(
                            channel_id,
                            "UpdateChannelError: invalid-channel-id (no downstream_id mapping)"
                        );
                        return Err(JDCError::DownstreamNotFoundWithChannelId(channel_id));
                    }
                };

                if let Some(downstream) = channel_manager_data.downstream.get_mut(&downstream_id) {
                    messages.extend_from_slice(&downstream.downstream_data.super_safe_lock(
                        |data| {
                            let mut messages: Vec<RouteMessageTo> = vec![];

                            let build_error = |code: &str| {
                                error!(channel_id, error_code = code, "UpdateChannelError");
                                Mining::UpdateChannelError(UpdateChannelError {
                                    channel_id,
                                    error_code: code
                                        .to_string()
                                        .try_into()
                                        .expect("valid error code"),
                                })
                            };

                            if let Some(standard_channel) =
                                data.standard_channels.get_mut(&channel_id)
                            {
                                let update_channel = standard_channel.update_channel(
                                    new_nominal_hash_rate,
                                    Some(requested_maximum_target.into()),
                                );
                                let new_target = standard_channel.get_target().clone();

                                if let Err(e) = update_channel {
                                    error!(channel_id, ?e, "StandardChannel update failed");

                                    let err_code = match e {
                                        StandardChannelError::InvalidNominalHashrate => {
                                            "invalid-nominal-hashrate"
                                        }
                                        StandardChannelError::RequestedMaxTargetOutOfRange => {
                                            "requested-max-target-out-of-range"
                                        }
                                        _ => "internal-error",
                                    };
                                    if err_code == "internal-error" {
                                        warn!("Failed to update extended channel {channel_id}");
                                    } else {
                                        return vec![(downstream_id, build_error(err_code)).into()];
                                    }
                                }

                                messages.push(
                                    (
                                        downstream_id,
                                        Mining::SetTarget(SetTarget {
                                            channel_id,
                                            maximum_target: new_target.into(),
                                        }),
                                    )
                                        .into(),
                                );
                            } else if let Some(extended_channel) =
                                data.extended_channels.get_mut(&channel_id)
                            {
                                let update_channel = extended_channel.update_channel(
                                    new_nominal_hash_rate,
                                    Some(requested_maximum_target.into()),
                                );
                                let new_target = extended_channel.get_target().clone();

                                if let Err(e) = update_channel {
                                    error!(channel_id, ?e, "StandardChannel update failed");
                                    let err_code = match e {
                                        ExtendedChannelError::InvalidNominalHashrate => {
                                            "invalid-nominal-hashrate"
                                        }
                                        ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                                            "requested-max-target-out-of-range"
                                        }
                                        _ => "internal-error",
                                    };
                                    if err_code == "internal-error" {
                                        warn!("Failed to update extended channel {channel_id}");
                                    } else {
                                        return vec![(downstream_id, build_error(err_code)).into()];
                                    }
                                }

                                messages.push(
                                    (
                                        downstream_id,
                                        Mining::SetTarget(SetTarget {
                                            channel_id,
                                            maximum_target: new_target.into(),
                                        }),
                                    )
                                        .into(),
                                );
                            } else {
                                error!("UpdateChannelError: invalid-channel-id");
                                return vec![
                                    (downstream_id, build_error("invalid-channel-id")).into()
                                ];
                            }

                            messages
                        },
                    ));
                }

                let mut downstream_hashrate = 0.0;
                let mut min_target: Target = [0xff; 32].into();

                for (_, downstream) in channel_manager_data.downstream.iter() {
                    downstream.downstream_data.super_safe_lock(|data| {
                        let mut update_from_channel = |hashrate: f32, target: &Target| {
                            downstream_hashrate += hashrate;
                            min_target = std::cmp::min(target.clone(), min_target.clone());
                        };

                        for (_, channel) in data.standard_channels.iter() {
                            update_from_channel(
                                channel.get_nominal_hashrate(),
                                channel.get_target(),
                            );
                        }

                        for (_, channel) in data.extended_channels.iter() {
                            update_from_channel(
                                channel.get_nominal_hashrate(),
                                channel.get_target(),
                            );
                        }
                    });
                }

                if let Some(ref upstream_channel) = channel_manager_data.upstream_channel {
                    debug!(
                        "Checking upstream channel {} with hashrate {} and target {:?}",
                        upstream_channel.get_channel_id(),
                        upstream_channel.get_nominal_hashrate(),
                        upstream_channel.get_target()
                    );

                    info!("Sending update channel message upstream");
                    messages.push(
                        Mining::UpdateChannel(UpdateChannel {
                            channel_id: upstream_channel.get_channel_id(),
                            nominal_hash_rate: downstream_hashrate,
                            maximum_target: min_target.into(),
                        })
                        .into(),
                    )
                }

                Ok(messages)
            })?;

        for messages in messages {
            messages.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    // Handles a `SubmitSharesStandard` message from a downstream.
    //
    // Steps:
    // 1. Validate the share against the downstream channel.
    //    - On error, respond with `SubmitSharesError`.
    //    - On success, acknowledge with `SubmitSharesSuccess` (and optionally a block found).
    //
    // 2. If the share is valid, attempt to forward it upstream:
    //    - Translate the share into an upstream `SubmitSharesExtended`.
    //    - Validate with the upstream channel.
    //    - Forward valid shares (or block solutions) upstream.
    async fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesStandard");
        let channel_id = msg.channel_id;
        let job_id = msg.job_id;

        let build_error = |code: &str| {
            Mining::SubmitSharesError(SubmitSharesError {
                channel_id,
                sequence_number: msg.sequence_number,
                error_code: code.to_string().try_into().expect("valid error code"),
            })
        };

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream_id) = channel_manager_data.channel_id_to_downstream_id.get(&channel_id) else {
                warn!("No downstream_id found for channel_id={channel_id}");
                return Err(JDCError::DownstreamNotFoundWithChannelId(channel_id))
            };
            let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id) else {
                warn!("No downstream found for downstream_id={downstream_id}");
                return Err(JDCError::DownstreamNotFound(*downstream_id));
            };
            let Some(prev_hash) = channel_manager_data.last_new_prev_hash.as_ref() else {
                warn!("No prev_hash available yet, ignoring share");
                return Err(JDCError::LastNewPrevhashNotFound);
            };

            downstream.downstream_data.super_safe_lock(|data| {
                let mut messages: Vec<RouteMessageTo> = vec![];

                let Some(standard_channel) = data.standard_channels.get_mut(&channel_id) else {
                    error!("SubmitSharesError: channel_id: {channel_id}, sequence_number: {}, error_code: invalid-channel-id", msg.sequence_number);
                    return Ok(vec![(*downstream_id, build_error("invalid-channel-id")).into()]);
                };

                let Some(vardiff) = channel_manager_data.vardiff.get_mut(&(channel_id, *downstream_id)) else {
                    return Err(JDCError::VardiffNotFound(channel_id));
                };
                vardiff.increment_shares_since_last_update();
                let res = standard_channel.validate_share(msg.clone());
                let mut is_downstream_share_valid = false;
                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesStandard on downstream channel: valid share | channel_id: {}, sequence_number: {} ☑️",
                            channel_id, msg.sequence_number
                        );
                        is_downstream_share_valid = true;
                    }
                    Ok(ShareValidationResult::ValidWithAcknowledgement(
                        last_sequence_number,
                        new_submits_accepted_count,
                        new_shares_sum,
                    )) => {
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number,
                            new_submits_accepted_count,
                            new_shares_sum,
                        };
                        is_downstream_share_valid = true;
                        info!("SubmitSharesStandard on downstream channel: {} ✅", success);
                        messages.push(
                            (downstream.downstream_id,
                            Mining::SubmitSharesSuccess(success)).into(),
                        );
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesStandard on downstream channel: 💰 Block Found!!! 💰");
                        is_downstream_share_valid = true;
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesStandard: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into()?,
                            };

                            messages.push(TemplateDistribution::SubmitSolution(solution.clone()).into());
                        }
                        let share_accounting = standard_channel.get_share_accounting().clone();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        messages.push((
                            downstream.downstream_id,
                            Mining::SubmitSharesSuccess(success),
                        ).into());
                    }
                    Err(err) => {
                        let code = match err {
                            ShareValidationError::Invalid => "invalid-share",
                            ShareValidationError::Stale => "stale-share",
                            ShareValidationError::InvalidJobId => "invalid-job-id",
                            ShareValidationError::DoesNotMeetTarget => "difficulty-too-low",
                            ShareValidationError::DuplicateShare => "duplicate-share",
                            _ => unreachable!(),
                        };
                        error!("❌ SubmitSharesError: ch={}, seq={}, error={code}", channel_id, msg.sequence_number);
                        messages.push((*downstream_id, build_error(code)).into());
                    }
                }

                if !is_downstream_share_valid {
                    return Ok(messages);
                }

                if let Some(upstream_channel) = channel_manager_data.upstream_channel.as_mut() {
                    let prefix = standard_channel.get_extranonce_prefix().clone();
                    let mut extranonce_parts = Vec::new();
                    let up_prefix = upstream_channel.get_extranonce_prefix();
                    extranonce_parts.extend_from_slice(&prefix[up_prefix.len()..]);

                    let upstream_message = channel_manager_data
                    .downstream_channel_id_and_job_id_to_template_id
                    .get(&(channel_id, job_id))
                    .and_then(|tid| channel_manager_data.template_id_to_upstream_job_id.get(tid))
                    .map(|&upstream_job_id| {
                        SubmitSharesExtended {
                            channel_id: upstream_channel.get_channel_id(),
                            job_id: upstream_job_id as u32,
                            extranonce: extranonce_parts.try_into().unwrap(),
                            nonce: msg.nonce,
                            ntime: msg.ntime,
                            // We assign sequence number later, when we validate the share
                            // and send it to upstream.
                            sequence_number: 0,
                            version: msg.version,
                        }
                    });

                    if let Some(mut upstream_message) = upstream_message {
                        let res = upstream_channel.validate_share(upstream_message.clone());
                        match res {
                            Ok(client::share_accounting::ShareValidationResult::Valid) => {
                                upstream_message.sequence_number = channel_manager_data.sequence_number_factory.next();
                                info!(
                                    "SubmitSharesStandard, forwarding it to upstream: valid share | channel_id: {}, sequence_number: {}  ✅",
                                    channel_id, upstream_message.sequence_number
                                );
                                messages.push(Mining::SubmitSharesExtended(upstream_message).into());
                            }
                            Ok(client::share_accounting::ShareValidationResult::BlockFound) => {
                                upstream_message.sequence_number = channel_manager_data.sequence_number_factory.next();
                                info!("SubmitSharesStandard forwarding it to upstream: 💰 Block Found!!! 💰");
                                let push_solution = PushSolution {
                                    extranonce: standard_channel.get_extranonce_prefix().to_vec().try_into()?,
                                    ntime: upstream_message.ntime,
                                    nonce: upstream_message.nonce,
                                    version: upstream_message.version,
                                    nbits: prev_hash.n_bits,
                                    prev_hash: prev_hash.prev_hash.clone(),
                                };
                                messages.push(JobDeclaration::PushSolution(push_solution).into());
                                messages.push(Mining::SubmitSharesExtended(upstream_message).into());
                            }
                            Err(err) => {
                                let code = match err {
                                    client::share_accounting::ShareValidationError::Invalid => "invalid-share",
                                    client::share_accounting::ShareValidationError::Stale => "stale-share",
                                    client::share_accounting::ShareValidationError::InvalidJobId => "invalid-job-id",
                                    client::share_accounting::ShareValidationError::DoesNotMeetTarget => "difficulty-too-low",
                                    client::share_accounting::ShareValidationError::DuplicateShare => "duplicate-share",
                                    _ => unreachable!(),
                                };
                                debug!("❌ SubmitSharesError not forwarding it to upstream: ch={}, seq={}, error={code}", channel_id, upstream_message.sequence_number);
                            }
                        }
                    }
                }

                Ok(messages)
            })
        })?;

        for messages in messages {
            messages.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    // Handles a `SubmitSharesExtended` message from a downstream.
    //
    // Steps:
    // 1. Validate the share against the downstream channel.
    //    - On error, respond with `SubmitSharesError`.
    //    - On success, acknowledge with `SubmitSharesSuccess` (and optionally a block found).
    //
    // 2. If the share is valid, attempt to forward it upstream:
    //    - Translate the share into an upstream `SubmitSharesExtended`.
    //    - Validate with the upstream channel.
    //    - Forward valid shares (or block solutions) upstream.
    async fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended<'_>,
    ) -> Result<(), Self::Error> {
        info!("Received SubmitSharesExtended");
        let channel_id = msg.channel_id;
        let job_id = msg.job_id;

        let build_error = |code: &str| {
            Mining::SubmitSharesError(SubmitSharesError {
                channel_id,
                sequence_number: msg.sequence_number,
                error_code: code.to_string().try_into().expect("valid error code"),
            })
        };

        let messages = self.channel_manager_data.super_safe_lock(|channel_manager_data| {
            let Some(downstream_id) = channel_manager_data.channel_id_to_downstream_id.get(&channel_id) else {
                warn!("No downstream_id found for channel_id={channel_id}");
                return Err(JDCError::DownstreamNotFoundWithChannelId(channel_id));
            };
            let Some(downstream) = channel_manager_data.downstream.get_mut(downstream_id) else {
                warn!("No downstream found for downstream_id={downstream_id}");
                return Err(JDCError::DownstreamNotFound(*downstream_id));
            };
            let Some(prev_hash) = channel_manager_data.last_new_prev_hash.as_ref() else {
                warn!("No prev_hash available yet, ignoring share");
                return Err(JDCError::LastNewPrevhashNotFound);
            };
            downstream.downstream_data.super_safe_lock(|data| {
                let mut messages: Vec<RouteMessageTo> = vec![];

                let Some(extended_channel) = data.extended_channels.get_mut(&channel_id) else {
                    error!("SubmitSharesError: channel_id: {channel_id}, sequence_number: {}, error_code: invalid-channel-id", msg.sequence_number);
                    return Ok(vec![(*downstream_id, build_error("invalid-channel-id")).into()]);
                };

                let Some(vardiff) = channel_manager_data.vardiff.get_mut(&(channel_id, *downstream_id)) else {
                    return Err(JDCError::VardiffNotFound(channel_id));
                };
                vardiff.increment_shares_since_last_update();
                let res = extended_channel.validate_share(msg.clone());
                let mut is_downstream_share_valid = false;
                match res {
                    Ok(ShareValidationResult::Valid) => {
                        info!(
                            "SubmitSharesExtended on downstream channel: valid share | channel_id: {}, sequence_number: {} ☑️",
                            channel_id, msg.sequence_number
                        );
                        is_downstream_share_valid = true;
                    }
                    Ok(ShareValidationResult::ValidWithAcknowledgement(
                        last_sequence_number,
                        new_submits_accepted_count,
                        new_shares_sum,
                    )) => {
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number,
                            new_submits_accepted_count,
                            new_shares_sum,
                        };
                        info!("SubmitSharesExtended on downstream channel: {} ✅", success);
                        is_downstream_share_valid = true;
                        messages.push((
                            downstream.downstream_id,
                            Mining::SubmitSharesSuccess(success),
                        ).into());
                    }
                    Ok(ShareValidationResult::BlockFound(template_id, coinbase)) => {
                        info!("SubmitSharesExtended on downstream channel: 💰 Block Found!!! 💰");
                        if let Some(template_id) = template_id {
                            info!("SubmitSharesExtended: Propagating solution to the Template Provider.");
                            let solution = SubmitSolution {
                                template_id,
                                version: msg.version,
                                header_timestamp: msg.ntime,
                                header_nonce: msg.nonce,
                                coinbase_tx: coinbase.try_into()?,
                            };
                            messages.push(TemplateDistribution::SubmitSolution(solution.clone()).into());
                        }
                        let share_accounting = extended_channel.get_share_accounting().clone();
                        let success = SubmitSharesSuccess {
                            channel_id,
                            last_sequence_number: share_accounting.get_last_share_sequence_number(),
                            new_submits_accepted_count: share_accounting.get_shares_accepted(),
                            new_shares_sum: share_accounting.get_share_work_sum(),
                        };
                        is_downstream_share_valid = true;
                        messages.push((
                            downstream.downstream_id,
                            Mining::SubmitSharesSuccess(success),
                        ).into());
                    }
                    Err(err) => {
                        let code = match err {
                            ShareValidationError::Invalid => "invalid-share",
                            ShareValidationError::Stale => "stale-share",
                            ShareValidationError::InvalidJobId => "invalid-job-id",
                            ShareValidationError::DoesNotMeetTarget => "difficulty-too-low",
                            ShareValidationError::DuplicateShare => "duplicate-share",
                            _ => unreachable!(),
                        };
                        error!("❌ SubmitSharesError on downstream channel: ch={}, seq={}, error={code}", channel_id, msg.sequence_number);
                        messages.push((*downstream_id, build_error(code)).into());
                    }
                }

                if !is_downstream_share_valid{
                    return Ok(messages);
                }

                if let Some(upstream_channel) = channel_manager_data.upstream_channel.as_mut() {
                    let prefix = extended_channel.get_extranonce_prefix().clone();
                    let mut extranonce_parts = Vec::new();
                    let up_prefix = upstream_channel.get_extranonce_prefix();
                    extranonce_parts.extend_from_slice(&prefix[up_prefix.len()..]);

                    let upstream_message = channel_manager_data
                    .downstream_channel_id_and_job_id_to_template_id
                    .get(&(channel_id, job_id))
                    .and_then(|tid| channel_manager_data.template_id_to_upstream_job_id.get(tid))
                    .map(|&upstream_job_id| {
                        let mut new_msg = msg.clone();
                        new_msg.channel_id = upstream_channel.get_channel_id();
                        new_msg.job_id = upstream_job_id as u32;
                        // We assign sequence number later, when we validate the share
                        // and send it to upstream.
                        new_msg.sequence_number = 0;

                        extranonce_parts.extend_from_slice(&msg.extranonce.to_vec());
                        new_msg.extranonce = extranonce_parts.try_into().unwrap();

                        new_msg
                    });
                    if let Some(mut upstream_message) = upstream_message{
                        let res = upstream_channel.validate_share(upstream_message.clone());
                        match res {
                            Ok(client::share_accounting::ShareValidationResult::Valid) => {
                                upstream_message.sequence_number = channel_manager_data.sequence_number_factory.next();
                                info!(
                                    "SubmitSharesExtended forwarding it to upstream: valid share | channel_id: {}, sequence_number: {}  ✅",
                                    channel_id, upstream_message.sequence_number
                                );
                                messages.push(
                                    Mining::SubmitSharesExtended(upstream_message.into_static()).into(),
                                );
                            }
                            Ok(client::share_accounting::ShareValidationResult::BlockFound) => {
                                upstream_message.sequence_number = channel_manager_data.sequence_number_factory.next();
                                info!("SubmitSharesExtended forwarding it to upstream: 💰 Block Found!!! 💰");
                                let mut channel_extranonce = upstream_channel.get_extranonce_prefix().to_vec();
                                channel_extranonce.extend_from_slice(&upstream_message.extranonce.to_vec());
                                let push_solution = PushSolution {
                                    extranonce: channel_extranonce.try_into()?,
                                    ntime: upstream_message.ntime,
                                    nonce: upstream_message.nonce,
                                    version: upstream_message.version,
                                    nbits: prev_hash.n_bits,
                                    prev_hash: prev_hash.prev_hash.clone(),
                                };
                                messages.push(JobDeclaration::PushSolution(push_solution.clone()).into());
                                messages.push(Mining::SubmitSharesExtended(upstream_message.into_static()).into());
                            }
                            Err(err) => {
                                let code = match err {
                                    client::share_accounting::ShareValidationError::Invalid=>"invalid-share",
                                    client::share_accounting::ShareValidationError::Stale=>"stale-share",
                                    client::share_accounting::ShareValidationError::InvalidJobId=>"invalid-job-id",
                                    client::share_accounting::ShareValidationError::DoesNotMeetTarget=>"difficulty-too-low",
                                    client::share_accounting::ShareValidationError::DuplicateShare=>"duplicate-share",
                                    _ => unreachable!(),
                                };
                                debug!("❌ SubmitSharesError not forwarding it to upstream: ch={}, seq={}, error={code}", channel_id, upstream_message.sequence_number);
                            }
                        }
                    }
                }

                Ok(messages)
            })
        })?;

        for messages in messages {
            messages.forward(&self.channel_manager_channel).await;
        }

        Ok(())
    }

    // Handles an incoming `SetCustomMiningJob` message from a downstream.
    async fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob<'_>,
    ) -> Result<(), Self::Error> {
        warn!("Received: {}", msg);
        Err(Self::Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
        ))
    }
}
