use crate::{
    downstream_sv1::Downstream,
    upstream_sv2::{EitherFrame, Message, StdFrame, UpstreamConnection},
    ProxyResult,
};
use async_channel::{Receiver, Sender};
use async_std::{net::TcpStream, task};
use binary_sv2::u256_from_int;
use codec_sv2::{Frame, HandshakeRole, Initiator};
use network_helpers::Connection;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::{
        common::{ParseUpstreamCommonMessages, SendTo as SendToCommon},
        mining::{ParseUpstreamMiningMessages, SendTo},
    },
    mining_sv2::{
        NewExtendedMiningJob, OpenExtendedMiningChannel, SetNewPrevHash, SubmitSharesExtended,ExtendedExtranonce,OpenExtendedMiningChannelSuccess, Extranonce

    },
    parsers::Mining,
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
    utils::Mutex,
};
use std::{net::SocketAddr, sync::Arc};

#[derive(Debug, Clone)]
pub struct Upstream {
    channel_id: Option<u32>,
    connection: UpstreamConnection,
    submit_from_dowstream: Receiver<SubmitSharesExtended<'static>>,
    new_prev_hash_sender: Sender<SetNewPrevHash<'static>>,
    new_extended_mining_job_sender: Sender<NewExtendedMiningJob<'static>>,
    extranonce_sender: Sender<ExtendedExtranonce>,
    /// Minimum `extranonce2` size. Initially requested in the `proxy-config.toml`, and ultimately
    /// set by the SV2 Upstream via the SV2 `OpenExtendedMiningChannelSuccess` message.
    pub min_extranonce_size: u16,
}

impl Upstream {
    /// Instantiate a new `Upstream`.
    /// Connect to the SV2 Upstream role (most typically a SV2 Pool). Initializes the
    /// `UpstreamConnection` with a channel to send and receive messages from the SV2 Upstream
    /// role, and uses a channel provided in the function arguments to send and receive messages
    /// from the Downstream Translator Proxy.
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        submit_from_dowstream: Receiver<SubmitSharesExtended<'static>>,
        new_prev_hash_sender: Sender<SetNewPrevHash<'static>>,
        new_extended_mining_job_sender: Sender<NewExtendedMiningJob<'static>>,
        min_extranonce_size: u16,
        extranonce_sender: Sender<ExtendedExtranonce>,
    ) -> ProxyResult<Arc<Mutex<Self>>> {
        // Connect to the SV2 Upstream role
        let socket = TcpStream::connect(address).await?;
        let initiator = Initiator::from_raw_k(authority_public_key)?;

        println!(
            "\nPROXY SERVER - ACCEPTING FROM UPSTREAM: {}\n",
            socket.peer_addr()?
        );

        // Channel to send and receive messages to the SV2 Upstream role
        let (receiver, sender) =
            Connection::new(socket, HandshakeRole::Initiator(initiator), 10).await;
        // Initialize `UpstreamConnection` with channel for SV2 Upstream role communication and
        // channel for downstream Translator Proxy communication
        let connection = UpstreamConnection { receiver, sender };

        Ok(Arc::new(Mutex::new(Self {
            connection,
            submit_from_dowstream,
            new_prev_hash_sender,
            new_extended_mining_job_sender,
            channel_id: None,
            min_extranonce_size,
            extranonce_sender,
        })))
    }

    /// Setups the connection with the SV2 Upstream role (Pool)
    pub async fn connect(
        self_: Arc<Mutex<Self>>,
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<()> {
        // Get the `SetupConnection` message with Mining Device information (currently hard coded)
        let setup_connection = Self::get_setup_connection_message(min_version, max_version)?;
        let mut connection = self_.safe_lock(|s| s.connection.clone()).unwrap();

        // Put the `SetupConnection` message in a `StdFrame` to be sent over the wire
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into()?;
        // Send the `SetupConnection` frame to the SV2 Upstream role
        // We support only one upstream if is not possible to connect we can just panic and let the
        // user know which is the issue
        connection.send(sv2_frame).await.unwrap();

        // Wait for the SV2 Upstream to respond with either a `SetupConnectionSuccess` or a
        // `SetupConnectionError` inside a SV2 binary message frame
        let mut incoming: StdFrame = connection.receiver.recv().await.unwrap().try_into()?;
        // Gets the binary frame message type from the message header
        let message_type = incoming.get_header().unwrap().msg_type();
        // Gets the message payload
        let payload = incoming.payload();

        // Handle the incoming message (should be either `SetupConnectionSuccess` or
        // `SetupConnectionError`)
        ParseUpstreamCommonMessages::handle_message_common(
            self_.clone(),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )?;

        // Send open channel request before returning
        let user_identity = "ABC".to_string().try_into()?;
        let min_extranonce_size = self_.safe_lock(|s| s.min_extranonce_size).unwrap();
        println!("\n MIN EXTSIZE: {:?}", &min_extranonce_size);
        let open_channel = Mining::OpenExtendedMiningChannel(OpenExtendedMiningChannel {
            request_id: 0.into(),               // TODO
            user_identity,                      // TODO
            nominal_hash_rate: 5.4,             // TODO
            max_target: u256_from_int(567_u64), // TODO
            min_extranonce_size,
        });
        let sv2_frame: StdFrame = Message::Mining(open_channel).try_into()?;
        connection.send(sv2_frame).await.unwrap();
        Ok(())
    }

    /// Parse the incoming SV2 message from the Upstream role and use the
    /// `Upstream.sender_downstream` to send the message to the
    /// `Translator.upstream_translator.receiver` to be handled.
    pub fn parse_incoming(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                // Waiting to receive a message from the SV2 Upstream role
                let recv = self_.safe_lock(|s| s.connection.receiver.clone()).unwrap();
                let incoming = recv.recv().await.unwrap();
                let mut incoming: StdFrame = incoming
                    .try_into()
                    .expect("Err converting received frame into `StdFrame`");
                println!("TU RECV SV2 FROM UPSTREAM: {:?}", &incoming);
                // On message receive, get the message type from the message header and get the
                // message payload
                let message_type = incoming
                    .get_header()
                    .expect("UnexpectedNoiseFrame: Expected `SV2Frame`, received `NoiseFrame`")
                    .msg_type();
                let payload = incoming.payload();

                // Since this is not communicating with an SV2 proxy, but instead a custom SV1
                // proxy where the routing logic is handled via the `Upstream`'s communication
                // channels, we do not use the mining routing logic in the SV2 library and specify
                // no mining routing logic here.
                let routing_logic = MiningRoutingLogic::None;

                // Gets the response message for the received SV2 Upstream role message
                // `handle_message_mining` takes care of the SetupConnection +
                // SetupConnection.Success
                let next_message_to_send = Upstream::handle_message_mining(
                    self_.clone(),
                    message_type,
                    payload,
                    routing_logic,
                );
                // Routes the incoming messages accordingly
                match next_message_to_send {
                    // No translation required, simply respond to SV2 pool w a SV2 message
                    Ok(SendTo::Respond(message_for_upstream)) => {
                        println!("TU SEND DIRECTLY TO UPSTREAM: {:?}", &message_for_upstream);

                        let message = Message::Mining(message_for_upstream);
                        let frame: StdFrame = message
                            .try_into()
                            .expect("Err converting `Message::Mining` to `StdFrame`");
                        let frame: EitherFrame = frame
                            .try_into()
                            .expect("Err converting `StdFrame` to `EitherFrame`");

                        // Relay the response message to the Upstream role
                        let sender = self_
                            .safe_lock(|self_| self_.connection.sender.clone())
                            .unwrap();
                        sender.send(frame).await.unwrap();
                    }
                    // We use None as we do not send the message to anyone but just use it
                    // internally so SendTo::None have the right semantic
                    Ok(SendTo::None(Some(m))) => {
                        match m {
                            Mining::OpenExtendedMiningChannelSuccess(m) => {
                                let extranonce_len = crate::EXTRNONCE_LEN;
                                let min_extranonce_size = self_.safe_lock(|s|s.min_extranonce_size).unwrap();

                                let extranonce: Vec<u8> = m.extranonce_prefix.inner_as_ref().to_vec();

                                if (m.extranonce_size as usize + extranonce.len()) != extranonce_len {
                                    panic!("size different than 32 not yet supported, size is {}, prefix len is {}", m.extranonce_size, extranonce.len())
                                };

                                let extranonce = Extranonce::from_vec_with_len(extranonce, extranonce_len);

                                let upstream_extrnonce_len = extranonce_len - min_extranonce_size as usize;
                                let self_extranonce_len = crate::SELF_EXTRNONCE_LEN;

                                let range_0 = 0..upstream_extrnonce_len;
                                let range_1 = upstream_extrnonce_len..upstream_extrnonce_len + self_extranonce_len;
                                let range_2 = upstream_extrnonce_len + self_extranonce_len..extranonce_len;
                                let extended = ExtendedExtranonce::from_extranonce(extranonce,range_0,range_1,range_2);
                                let sender = self_.safe_lock(|s| s.extranonce_sender.clone()).unwrap();
                                sender.send(extended).await.unwrap();
                            }
                            Mining::NewExtendedMiningJob(m) => {
                                let sender = self_
                                    .safe_lock(|s| s.new_extended_mining_job_sender.clone())
                                    .unwrap();
                                sender.send(m).await.unwrap();
                            }
                            Mining::SetNewPrevHash(m) => {
                                let sender =
                                    self_.safe_lock(|s| s.new_prev_hash_sender.clone()).unwrap();
                                sender.send(m).await.unwrap();
                            }
                            // impossible state
                            _ => panic!(),
                        }
                    }
                    Ok(SendTo::None(None)) => (),
                    // NO need to handle impossible state just panic cause are impossible and we
                    // will never panic ;-)
                    Ok(_) => panic!(),
                    Err(_) => todo!("Handle `SendTo` error on Upstream"),
                }
            }
        });
    }

    pub fn handle_submit(self_: Arc<Mutex<Self>>) {
        // TODO
        // check if submit meet the upstream target and if so send back (upstream target will
        // likely be not the same of downstream target)
        task::spawn(async move {
            loop {
                let receiver = self_
                    .safe_lock(|s| s.submit_from_dowstream.clone())
                    .unwrap();
                let mut sv2_submit: SubmitSharesExtended = receiver.recv().await.unwrap();
                sv2_submit.channel_id = self_
                    .safe_lock(|s| {
                        s.channel_id
                            .expect("Expected `Upstream`'s `channel_id` to be `Some`, got `None`")
                    })
                    .unwrap();

                let message = Message::Mining(
                    roles_logic_sv2::parsers::Mining::SubmitSharesExtended(sv2_submit),
                );

                let frame: StdFrame = message
                    .try_into()
                    .expect("Err converting `PoolMessage` to `StdFrame`");
                let frame: EitherFrame = frame
                    .try_into()
                    .expect("Err converting `StdFrame` to `EitherFrame`");
                let sender = self_
                    .safe_lock(|self_| self_.connection.sender.clone())
                    .unwrap();
                sender.send(frame).await.unwrap();
            }
        });
    }

    fn _is_contained_in_upstream_target(&self, _share: SubmitSharesExtended) -> bool {
        todo!()
    }

    /// Creates the `SetupConnection` message to setup the connection with the SV2 Upstream role.
    /// TODO: The Mining Device information is hard coded here, need to receive from Downstream
    /// instead.
    fn get_setup_connection_message(
        min_version: u16,
        max_version: u16,
    ) -> ProxyResult<SetupConnection<'static>> {
        let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
        let vendor = String::new().try_into()?;
        let hardware_version = String::new().try_into()?;
        let firmware = String::new().try_into()?;
        let device_id = String::new().try_into()?;
        let flags = 0b0000_0000_0000_0000_0000_0000_0000_1110;
        Ok(SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version,
            max_version,
            flags,
            endpoint_host,
            endpoint_port: 50,
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }
}

impl IsUpstream<Downstream, NullDownstreamMiningSelector> for Upstream {
    fn get_version(&self) -> u16 {
        todo!()
    }

    fn get_flags(&self) -> u32 {
        todo!()
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        todo!()
    }

    fn get_id(&self) -> u32 {
        todo!()
    }

    fn get_mapper(&mut self) -> Option<&mut roles_logic_sv2::common_properties::RequestIdMapper> {
        todo!()
    }

    fn get_remote_selector(&mut self) -> &mut NullDownstreamMiningSelector {
        todo!()
    }
}

impl IsMiningUpstream<Downstream, NullDownstreamMiningSelector> for Upstream {
    fn total_hash_rate(&self) -> u64 {
        todo!()
    }

    fn add_hash_rate(&mut self, _to_add: u64) {
        todo!()
    }

    fn get_opened_channels(
        &mut self,
    ) -> &mut Vec<roles_logic_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, _c: roles_logic_sv2::common_properties::UpstreamChannel) {
        todo!()
    }
}

impl ParseUpstreamCommonMessages<NoRouting> for Upstream {
    fn handle_setup_connection_success(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendToCommon, roles_logic_sv2::errors::Error> {
        Ok(SendToCommon::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<SendToCommon, roles_logic_sv2::errors::Error> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<SendToCommon, roles_logic_sv2::errors::Error> {
        todo!()
    }
}

impl ParseUpstreamMiningMessages<Downstream, NullDownstreamMiningSelector, NoRouting> for Upstream {
    fn get_channel_type(&self) -> roles_logic_sv2::handlers::mining::SupportedChannelTypes {
        roles_logic_sv2::handlers::mining::SupportedChannelTypes::Extended
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    /// SV2 `OpenStandardMiningChannelSuccess` message is NOT handled because it is NOT used for
    /// the Translator Proxy as only Extended channels are used between the Translator Proxy and
    /// the SV2 Upstream role.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
        _remote: Option<Arc<Mutex<Downstream>>>,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        println!("\n\nRR IN OEMCS\n\n");
        let extranonce_size_bytes = m.extranonce_size / 8;
        if self.min_extranonce_size < extranonce_size_bytes {
            panic!(
                "Proxy requested min extranonce size of {}, but pool requires min of {}",
                self.min_extranonce_size, m.extranonce_size
            );
        }
        // Set the `min_extranonce_size` in accordance to the SV2 Pool
        self.min_extranonce_size = m.extranonce_size;

        self.channel_id = Some(m.channel_id);
        let m = Mining::OpenExtendedMiningChannelSuccess(OpenExtendedMiningChannelSuccess {
            request_id: m.request_id,
            channel_id: m.channel_id,
            target: m.target.into_static(),
            extranonce_size: m.extranonce_size,
            extranonce_prefix: m.extranonce_prefix.into_static(),
        });
        Ok(SendTo::None(Some(m)))
    }

    fn handle_open_mining_channel_error(
        &mut self,
        _: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    /// Handle SV2 `UpdateChannelError`.
    /// TODO: Not implemented for demo.
    fn handle_update_channel_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    /// Handle SV2 `CloseChannel`.
    /// TODO: Not implemented for demo.
    fn handle_close_channel(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_submit_shares_success(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        // // TODO
        // let message = Mining::SetExtranoncePrefix(SetExtranoncePrefix {
        //     // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
        //     // jobs by the connection. Can be extended of standard channel (always extended for SV1
        //     // Translator Proxy)
        //     channel_id: m.channel_id,
        //     // Bytes used as implicit first part of extranonce.
        //     extranonce_prefix: m.extranonce_prefix.clone().into_static(),
        // });
        // Ok(SendTo::Respond(message))
        Ok(SendTo::None(None))
    }

    fn handle_submit_shares_error(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        // TODO
        // let message = Mining::SubmitSharesError(SubmitSharesError {
        //     // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
        //     // jobs by the connection. Can be extended of standard channel (always extended for SV1
        //     // Translator Proxy)
        //     channel_id: m.channel_id,
        //     // Sequence number
        //     sequence_number: m.sequence_number,
        //     // Relevant error reason code
        //     error_code: m.error_code.clone().into_static(),
        // });
        // Ok(SendTo::Respond(message))
        Ok(SendTo::None(None))
    }

    /// SV2 `NewMiningJob` message is NOT handled because it is NOT used for the Translator Proxy
    /// as only Extended channels are used between the Translator Proxy and the SV2 Upstream role.
    fn handle_new_mining_job(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::NewMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// Relay incoming `NewExtendedMiningJob` message to `Translator` to be handled. `Translator`
    /// will store this message until it receives a `SetNewPrevHash` message from the Upstream
    /// role. `Translator` will then format these messages into a SV1 `mining.notify` message to be
    /// sent to the Downstream role.
    fn handle_new_extended_mining_job(
        &mut self,
        m: roles_logic_sv2::mining_sv2::NewExtendedMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::NewExtendedMiningJob(NewExtendedMiningJob {
            // Extended channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection
            channel_id: m.channel_id,
            job_id: m.job_id,
            future_job: m.future_job, // Maybe hard code to false for demo
            version: m.version,
            version_rolling_allowed: m.version_rolling_allowed,
            merkle_path: m.merkle_path.clone().into_static(),
            coinbase_tx_prefix: m.coinbase_tx_prefix.clone().into_static(),
            coinbase_tx_suffix: m.coinbase_tx_suffix.clone().into_static(),
        });
        Ok(SendTo::None(Some(message)))
    }

    /// Relay incoming `SetNewPrevHash` message to `Translator` to be handled. `SetNewPrevHash`
    /// will be combined with the previously stored `NewExtendedMiningJob` message held by
    /// `Translator`, then formatted into a SV1 `mining.notify` message to be sent to the
    /// Downstream role.
    fn handle_set_new_prev_hash(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::SetNewPrevHash(SetNewPrevHash {
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            job_id: m.job_id,
            prev_hash: m.prev_hash.clone().into_static(),
            min_ntime: m.min_ntime,
            nbits: m.nbits,
        });
        Ok(SendTo::None(Some(message)))
    }

    /// Handle SV2 `SetCustomMiningJobSuccess`.
    /// TODO: Not implemented for demo.
    fn handle_set_custom_mining_job_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        unimplemented!()
    }

    /// Handle SV2 `SetCustomMiningJobError`.
    /// TODO: Not implemented for demo.
    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        unimplemented!()
    }

    /// Handle SV2 `SetTarget` message.
    /// RR: Not used in demo, target is hardcoded.
    fn handle_set_target(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        // let message = Mining::SetTarget(SetTarget {
        //     Channel identifier, stable for whole connection lifetime. Used for broadcasting new
        //     jobs by the connection. Can be extended of standard channel (always extended for SV1
        //     Translator Proxy)
        //     channel_id: m.channel_id,
        //     maximum_target: m.maximum_target.clone().into_static(),
        // });
        // Ok(SendTo::Respond(message))
        unimplemented!()
    }

    /// Handle SV2 `Reconnect` message.
    /// RR: Not used in demo
    fn handle_reconnect(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::Reconnect,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        unimplemented!()
    }
}
