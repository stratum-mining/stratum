use crate::sv1::downstream::DownstreamMessages;
use async_channel::Sender;
use roles_logic_sv2::mining_sv2::Target;
use tracing::debug;
use v1::{json_rpc, server_to_client, utils::HexU32Be};

#[derive(Debug, Clone)]
pub struct DownstreamData {
    pub channel_id: Option<u32>,
    pub downstream_id: u32,
    pub extranonce1: Vec<u8>,
    pub extranonce2_len: usize,
    pub version_rolling_mask: Option<HexU32Be>,
    pub version_rolling_min_bit: Option<HexU32Be>,
    pub last_job_version_field: Option<u32>,
    pub authorized_worker_names: Vec<String>,
    pub user_identity: String,
    pub valid_jobs: Vec<server_to_client::Notify<'static>>,
    pub target: Target,
    pub hashrate: f32,
    pub pending_set_difficulty: Option<json_rpc::Message>,
    pub pending_target: Option<Target>,
    pub pending_hashrate: Option<f32>,
    pub sv1_server_sender: Sender<DownstreamMessages>, // just here for time being
    pub first_set_difficulty_received: bool,
    // this is used to store the first notify message received in case it is received before the
    // first set_difficulty
    pub waiting_first_notify: Option<json_rpc::Message>,
}

impl DownstreamData {
    pub fn new(
        downstream_id: u32,
        target: Target,
        hashrate: f32,
        sv1_server_sender: Sender<DownstreamMessages>,
    ) -> Self {
        DownstreamData {
            channel_id: None,
            downstream_id,
            extranonce1: vec![0; 8],
            extranonce2_len: 4,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            last_job_version_field: None,
            authorized_worker_names: Vec::new(),
            user_identity: String::new(),
            valid_jobs: Vec::new(),
            target,
            hashrate,
            pending_set_difficulty: None,
            pending_target: None,
            pending_hashrate: None,
            sv1_server_sender,
            first_set_difficulty_received: false,
            waiting_first_notify: None,
        }
    }

    pub fn set_pending_target_and_hashrate(&mut self, new_target: Target, new_hashrate: f32) {
        self.pending_target = Some(new_target);
        self.pending_hashrate = Some(new_hashrate);
        debug!(
            "Downstream {}: Set pending target and hashrate",
            self.downstream_id
        );
    }
}
