use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    channel_logic::channel_factory::{ExtendedChannelKind, ProxyExtendedChannelFactory, Share},
    mining_sv2::{
        ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended, Target,
    },
    parsers::Mining,
    utils::{GroupId, Mutex},
};
use std::sync::Arc;
use tokio::sync::broadcast;
use v1::{client_to_server::Submit, server_to_client, utils::HexU32Be};

use super::super::{
    downstream::{DownstreamMessages, SetDownstreamTarget, SubmitShareWithChannelId},
    error::{
        Error::{self, PoisonLock},
        Result,
    },
    status,
};
use error_handling::handle_result;
use roles_logic_sv2::{channel_logic::channel_factory::OnNewShare, Error as RolesLogicError};
use tracing::{debug, error, info};

#[derive(Debug)]
pub struct Proxy {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    rx_sv1_downstream: Receiver<DownstreamMessages>,
    // /// Sends SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages to
    // /// the `Upstream`.
    // tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
    // /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` to be translated (along with a
    // /// SV2 `NewExtendedMiningJob` message) to a SV1 `mining.submit` for the `Downstream`.
    // rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    // /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream` to be translated (along
    // /// with a SV2 `SetNewPrevHash` message) to a SV1 `mining.submit` to be sent to the
    // /// `Downstream`.
    // rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    /// Sends SV1 `mining.notify` message (translated from the SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages stored in the `NextMiningNotify`) to the `Downstream`.
    tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    /// Allows the bridge the ability to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
    /// Stores the most recent SV1 `mining.notify` values to be sent to the `Downstream` upon
    /// receiving a new SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages **before** any
    /// Downstream role connects to the proxy.
    ///
    /// Once the proxy establishes a connection with the SV2 Upstream role, it immediately receives
    /// a SV2 `SetNewPrevHash` and `NewExtendedMiningJob` message. This happens before the
    /// connection to the Downstream role(s) occur. The `last_notify` member fields allows these
    /// first notify values to be relayed to the `Downstream` once a Downstream role connects. Once
    /// a Downstream role connects and receives the first notify values, this member field is no
    /// longer used.
    last_notify: Option<server_to_client::Notify<'static>>,
    pub(self) channel_factory: ProxyExtendedChannelFactory,
    future_jobs: Vec<NewExtendedMiningJob<'static>>,
    last_p_hash: Option<SetNewPrevHash<'static>>,
    target: Arc<Mutex<Vec<u8>>>,
    last_job_id: u32,
}
