use crate::{
    downstream_sv1::Downstream,
    upstream_sv2::{EitherFrame, StdFrame, UpstreamConnection},
};
use async_channel::{Receiver, Sender};
use async_std::net::TcpStream;
use codec_sv2::{Frame, HandshakeRole, Initiator};
use network_helpers::Connection;
use roles_logic_sv2::selectors::DownstreamMiningSelector;
use roles_logic_sv2::utils::Mutex;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    handlers::common::{ParseUpstreamCommonMessages, SendTo as SendToCommon},
    handlers::mining::{ParseUpstreamMiningMessages, SendTo},
    parsers::{MiningDeviceMessages, PoolMessages},
    routing_logic::{CommonRoutingLogic, MiningRoutingLogic, NoRouting},
    selectors::NullDownstreamMiningSelector,
};
use roles_logic_sv2::{
    common_properties::{IsMiningUpstream, IsUpstream},
    selectors::ProxyDownstreamMiningSelector,
};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug)]
pub struct Upstream {
    connection: UpstreamConnection,
    downstream_selector: ProxyDownstreamMiningSelector<Downstream>,
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
        let sv2_frame: StdFrame = PoolMessages::Common(setup_connection.into())
            .try_into()
            .unwrap();
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

        let downstream_selector = ProxyDownstreamMiningSelector::new();
        let self_ = Arc::new(Mutex::new(Self {
            connection,
            downstream_selector,
        }));

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

    fn parse_incoming(self_: Arc<Mutex<Self>>) {
        async_std::task::spawn(async move {
            loop {
                let recv = self_.safe_lock(|s| s.connection.receiver.clone()).unwrap();
                let mut incoming: StdFrame = recv.recv().await.unwrap().try_into().unwrap();
                let message_type = incoming.get_header().unwrap().msg_type();
                let payload = incoming.payload();
                let routing_logic = MiningRoutingLogic::None;

                let next_message_to_send = Upstream::handle_message_mining(
                    self_.clone(),
                    message_type,
                    payload,
                    routing_logic,
                );
                match next_message_to_send {
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

    fn add_hash_rate(&mut self, to_add: u64) {
        todo!()
    }

    fn get_opened_channels(
        &mut self,
    ) -> &mut Vec<roles_logic_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, c: roles_logic_sv2::common_properties::UpstreamChannel) {
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

    fn handle_open_standard_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
        remote: Option<Arc<Mutex<Downstream>>>,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_update_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_close_channel(
        &mut self,
        m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_new_mining_job(
        &mut self,
        m: roles_logic_sv2::mining_sv2::NewMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_new_extended_mining_job(
        &mut self,
        m: roles_logic_sv2::mining_sv2::NewExtendedMiningJob,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        // this does nothing rn
        // first thing the pool does is send a NewExtendedMiningJob
        Ok(SendTo::None(None))
    }

    fn handle_set_new_prev_hash(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetNewPrevHash,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        let downstreams = self
            .downstream_selector
            .get_downstreams_in_channel(m.channel_id)
            .unwrap();
        Ok(SendTo::RelaySameMessage(downstreams[0].clone()))
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_set_custom_mining_job_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_set_target(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }

    fn handle_reconnect(
        &mut self,
        m: roles_logic_sv2::mining_sv2::Reconnect,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, roles_logic_sv2::errors::Error>
    {
        todo!()
    }
}
