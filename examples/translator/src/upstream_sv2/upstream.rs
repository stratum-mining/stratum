use crate::{
    downstream_sv1::Downstream,
    upstream_sv2::{EitherFrame, Message, StdFrame, UpstreamConnection},
};
use async_channel::{Receiver, Sender};
use async_std::{net::TcpStream, task};
use codec_sv2::{Frame, HandshakeRole, Initiator};
use network_helpers::Connection;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::common::{ParseUpstreamCommonMessages, SendTo as SendToCommon},
    handlers::mining::{ParseUpstreamMiningMessages, SendTo},
    // mining_sv2::*,
    mining_sv2::{
        NewExtendedMiningJob, OpenExtendedMiningChannelSuccess, OpenMiningChannelError,
        SetExtranoncePrefix, SetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesSuccess,
    },
    parsers::Mining,
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::{NullDownstreamMiningSelector, ProxyDownstreamMiningSelector},
    utils::Mutex,
};
use std::{net::SocketAddr, sync::Arc};

#[derive(Debug)]
pub struct Upstream {
    connection: UpstreamConnection,
    /// TODO: Not used for demo, but needs to be properly implemented
    _downstream_selector: ProxyDownstreamMiningSelector<Downstream>,
}

impl Upstream {
    /// Instantiate a new `Upstream`.
    /// Connect to the SV2 Upstream role (most typically a SV2 Pool). Initialize the
    /// `UpstreamConnection` with a channel to send and receive messages to the SV2 Upstream role,
    /// and a channel to send and receive messages from the Downstream Translator Proxy.
    pub(crate) async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        sender_downstream: Sender<EitherFrame>,
        receiver_downstream: Receiver<EitherFrame>,
    ) -> Result<Arc<Mutex<Self>>, ()> {
        // Connect to the SV2 Upstream role
        let socket = TcpStream::connect(address).await.map_err(|_| ()).unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();

        println!(
            "\nPROXY SERVER - ACCEPTING FROM UPSTREAM: {}\n",
            socket.peer_addr().unwrap()
        );

        // Channel to send and receive messages to the SV2 Upstream role
        let (receiver, sender) =
            Connection::new(socket, HandshakeRole::Initiator(initiator), 10).await;
        // Initialize `UpstreamConnection` with channel for SV2 Upstream role communication and
        // channel for downstream Translator Proxy communication
        let connection = UpstreamConnection {
            sender,
            receiver,
            sender_downstream,
            receiver_downstream,
        };

        // Setup the connection with the SV2 Upstream role (Pool)
        let self_ = Self::setup_connection(connection).await.unwrap();
        Ok(self_)
    }

    /// Setups the connection with the SV2 Upstream role (Pool)
    async fn setup_connection(mut connection: UpstreamConnection) -> Result<Arc<Mutex<Self>>, ()> {
        // Get the `SetupConnection` message with Mining Device information (currently hard coded)
        let setup_connection = Self::get_setup_connection_message();

        // Put the `SetupConnection` message in a `StdFrame` to be sent over the wire
        let sv2_frame: StdFrame = Message::Common(setup_connection.into()).try_into().unwrap();
        // Send the `SetupConnection` frame to the SV2 Upstream role
        connection.send(sv2_frame).await.map_err(|_| ())?;

        // Wait for the SV2 Upstream to respond with either a `SetupConnectionSuccess` or a
        // `SetupConnectionError` inside a SV2 binary message frame
        let mut incoming: StdFrame = connection
            .receiver
            .recv()
            .await
            .unwrap()
            .try_into()
            .unwrap();
        // Gets the binary frame message type from the message header
        let message_type = incoming.get_header().unwrap().msg_type();
        // Gets the message payload
        let payload = incoming.payload();

        // TODO: Initialize an empty `ProxyDownstreamMiningSelector`, but should instead pass in a
        // `Downstream`
        let downstream_selector = ProxyDownstreamMiningSelector::new();
        let self_ = Arc::new(Mutex::new(Self {
            connection,
            _downstream_selector: downstream_selector,
        }));

        // // TODO: NOT HANDLED YET
        // // Receive messages from the downstream `Translator`
        // // RR: Think i need to refactor parse_incoming to receive the EitherFrame and handle
        // // appropriately. Make a new function called incoming_upstream to receive messages from
        // // upstream pool server + and another function called incoming_downstream to recieve
        // // messages from downstream proxy (basically does the next 2 lines)
        // // If these two lines are uncommented -> blocks and nothing works
        // let cloned = self_.clone();
        // let mut _incoming_downstream = task::spawn(async { Self::receive(cloned).await })
        //     .await
        //     .unwrap();

        Self::parse_incoming_downstream(self_.clone());

        // Handle the incoming message (should be either `SetupConnectionSuccess` or
        // `SetupConnectionError`)
        ParseUpstreamCommonMessages::handle_message_common(
            self_.clone(),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )
        .unwrap();

        Self::parse_incoming(self_.clone());
        Ok(self_)
    }

    /// Receive messages from the downstream `Translator::upstream_translator.sender`, sent in
    /// `UpstreamTranslator::send_sv2`.
    fn parse_incoming_downstream(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let recv = self_
                    .safe_lock(|s| s.connection.receiver_downstream.clone())
                    .unwrap();
                let message_incoming = recv.recv().await.unwrap();
                println!("TU RECV SV2 FROM TP: {:?}", &message_incoming);
                // message_incoming.try_into() // StdFrame
            }
        });
    }

    /// Parse the incoming SV2 message from the Upstream role and use the
    /// `Upstream.sender_downstream` to send the message to the
    /// `Translator.upstream_translator.receiver` to be handled.
    fn parse_incoming(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                // Waiting to receive a message from the SV2 Upstream role
                let recv = self_.safe_lock(|s| s.connection.receiver.clone()).unwrap();
                let mut incoming: StdFrame = recv.recv().await.unwrap().try_into().unwrap();
                println!("TU RECV SV2 FROM UPSTREAM: {:?}", &incoming);
                // On message receive, get the message type from the message header and get the
                // message payload
                let message_type = incoming.get_header().unwrap().msg_type();
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
                        let frame: StdFrame = message.try_into().unwrap();
                        let frame: EitherFrame = frame.try_into().unwrap();

                        // Relay the response message to the Upstream role
                        let sender = self_
                            .safe_lock(|self_| self_.connection.sender.clone())
                            .unwrap();
                        sender.send(frame).await.unwrap();
                    }
                    // Relay the SV2 message to `Translator.upstream_translator.receiver` via
                    // the `UpstreamConnection.downstream_sender`
                    Ok(SendTo::RelaySameMessageToSv1(message_to_translate)) => {
                        println!("\nTU SEND SV2 MSG TO TP: {:?}\n", &message_to_translate);
                        // Format message as `EitherFrame` to send to the
                        // `Translator.upstream_receiver`
                        let message_pool = Message::Mining(message_to_translate);
                        println!("\n\nMESSAGEPOOL: {:?}\n\n", &message_pool);
                        let message_frame: StdFrame = message_pool.try_into().unwrap();
                        let message: EitherFrame = message_frame.into();

                        // Get the `sender_downstream` and send the SV2 message to
                        // `Translator.receiver_upstream`
                        let sender = self_
                            .safe_lock(|self_| self_.connection.sender_downstream.clone())
                            .unwrap();
                        sender.send(message).await.unwrap();
                    }
                    // // No response is needed to be given to the SV2 Upstream role or the SV1
                    // // Downstream role
                    // Ok(SendTo::None(None)) => (),
                    // Ok(SendTo::RelayNewMessageToSv2(_, _))
                    // | Ok(SendTo::RelaySameMessageToSv2(_))
                    // | Ok(SendTo::Multiple(_)) => {
                    //     // /// Errors if a `SendTo::RelaySameMessageToSv2` or
                    //     // `SendTo::RelayNewMessageToSv2` request is made on SV1/SV2 application
                    //     // Error::UnsupportedRelayType,
                    //     //     // Proxy does not support this type
                    //     //     // Err(Error::ProxyDoesNotSupportMultiple
                    //     ()
                    // }
                    // Ok(SendTo::None(None)) => {
                    //     todo!("Handle None");
                    //     // Probably just end up putting ()
                    // }
                    // Ok(SendTo::None(Some(_))) => todo!("Handle SendTo::Some(Some(m))"),
                    Ok(_) => (),
                    Err(_) => (),
                }
            }
        });
    }

    /// Creates the `SetupConnection` message to setup the connection with the SV2 Upstream role.
    /// TODO: The Mining Device information is hard coded here, need to receive from Downstream
    /// instead.
    fn get_setup_connection_message() -> SetupConnection<'static> {
        let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into().unwrap();
        let vendor = String::new().try_into().unwrap();
        let hardware_version = String::new().try_into().unwrap();
        let firmware = String::new().try_into().unwrap();
        let device_id = String::new().try_into().unwrap();
        let flags = 0b0111_0000_0000_0000_0000_0000_0000_0000;
        SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version: 2,
            max_version: 2,
            flags,
            endpoint_host,
            endpoint_port: 50,
            vendor,
            hardware_version,
            firmware,
            device_id,
        }
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
        let message = Mining::OpenExtendedMiningChannelSuccess(OpenExtendedMiningChannelSuccess {
            // Client-specified request ID from OpenStandardMiningChannel message, so that the
            // client can pair responses with open channel requests.
            request_id: m.request_id,
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            // Initial target for the mining channel
            target: m.target.clone().into_static(),
            // Extranonce size (in bytes) set for the channel
            extranonce_size: m.extranonce_size,
            // Bytes used as implicit first part of extranonce
            extranonce_prefix: m.extranonce_prefix.clone().into_static(),
        });
        Ok(SendTo::Respond(message))
    }

    fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::OpenMiningChannelError(OpenMiningChannelError {
            // Client-specified request ID from OpenStandardMiningChannel message, so that the
            // client can pair responses with open channel requests.
            request_id: m.request_id,
            // Relevant error reason code
            error_code: m.error_code.clone().into_static(),
        });
        Ok(SendTo::Respond(message))
    }

    /// Handle SV2 `UpdateChannelError`.
    /// TODO: Not implemented for demo.
    fn handle_update_channel_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        unimplemented!()
    }

    /// Handle SV2 `CloseChannel`.
    /// TODO: Not implemented for demo.
    fn handle_close_channel(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        unimplemented!()
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::SetExtranoncePrefix(SetExtranoncePrefix {
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            // Bytes used as implicit first part of extranonce.
            extranonce_prefix: m.extranonce_prefix.clone().into_static(),
        });
        Ok(SendTo::Respond(message))
    }

    fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::SubmitSharesSuccess(SubmitSharesSuccess {
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            // Most recent sequence number with a correct result.
            last_sequence_number: m.last_sequence_number,
            // Count of new submits acknowledged within this batch.
            new_submits_accepted_count: m.new_submits_accepted_count,
            // Sum of shares acknowledged within this batch.
            new_shares_sum: m.new_shares_sum,
        });
        Ok(SendTo::Respond(message))
    }

    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::SubmitSharesError(SubmitSharesError {
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            // Sequence number
            sequence_number: m.sequence_number,
            // Relevant error reason code
            error_code: m.error_code.clone().into_static(),
        });
        Ok(SendTo::Respond(message))
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
        Ok(SendTo::RelaySameMessageToSv1(message))
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
        Ok(SendTo::RelaySameMessageToSv1(message))
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
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let message = Mining::SetTarget(SetTarget {
            // Channel identifier, stable for whole connection lifetime. Used for broadcasting new
            // jobs by the connection. Can be extended of standard channel (always extended for SV1
            // Translator Proxy)
            channel_id: m.channel_id,
            maximum_target: m.maximum_target.clone().into_static(),
        });
        Ok(SendTo::Respond(message))
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
