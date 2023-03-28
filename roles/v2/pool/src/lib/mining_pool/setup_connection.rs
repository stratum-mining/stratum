use crate::{
    error::{PoolError, PoolResult},
    EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use roles_logic_sv2::{
    common_messages_sv2::{
        has_requires_std_job, has_version_rolling, has_work_selection, SetupConnection,
        SetupConnectionSuccess,
    },
    common_properties::CommonDownstreamData,
    errors::Error,
    handlers::common::ParseDownstreamCommonMessages,
    parsers::{CommonMessages, PoolMessages},
    routing_logic::{CommonRoutingLogic, NoRouting},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tracing::{debug, error};

pub struct SetupConnectionHandler {
    header_only: Option<bool>,
}

impl SetupConnectionHandler {
    pub fn new() -> Self {
        Self { header_only: None }
    }
    pub async fn setup(
        self_: Arc<Mutex<Self>>,
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> PoolResult<CommonDownstreamData> {
        // read stdFrame from receiver

        let mut incoming: StdFrame = match receiver.recv().await {
            Ok(EitherFrame::Sv2(s)) => {
                debug!("Got sv2 message: {:?}", s);
                s
            }
            Ok(EitherFrame::HandShake(s)) => {
                error!(
                    "Got unexpected handshake message from upstream: {:?} at {}",
                    s, address
                );
                panic!()
            }
            Err(e) => {
                error!("Error receiving message: {:?}", e);
                return Err(Error::NoDownstreamsConnected.into());
            }
        };

        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();
        let response = ParseDownstreamCommonMessages::handle_message_common(
            self_.clone(),
            message_type,
            payload,
            CommonRoutingLogic::None,
        )?;

        let message = response.into_message().ok_or(PoolError::RolesLogic(
            roles_logic_sv2::Error::UnexpectedMessage(message_type),
        ))?;

        let sv2_frame: StdFrame = PoolMessages::Common(message.clone()).try_into()?;
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await?;
        self_.safe_lock(|s| s.header_only)?;

        match message {
            CommonMessages::SetupConnectionSuccess(m) => {
                debug!("Sent back SetupConnectionSuccess: {:?}", m);
                Ok(CommonDownstreamData {
                    header_only: has_requires_std_job(m.flags),
                    work_selection: has_work_selection(m.flags),
                    version_rolling: has_version_rolling(m.flags),
                })
            }
            _ => panic!(),
        }
    }
}

impl ParseDownstreamCommonMessages<NoRouting> for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
        _: Option<Result<(CommonDownstreamData, SetupConnectionSuccess), Error>>,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        use roles_logic_sv2::handlers::common::SendTo;
        let header_only = incoming.requires_standard_job();
        debug!("Handling setup connection: header_only: {}", header_only);
        self.header_only = Some(header_only);
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: incoming.flags,
                used_version: 2,
            }),
        ))
    }
}
