use std::{
    cell::RefCell,
    sync::{atomic::AtomicBool, Arc},
};
use stratum_common::roles_logic_sv2::{mining_sv2::Target, utils::Mutex};
use tracing::debug;
use v1::{json_rpc, utils::HexU32Be};

use super::SubmitShareWithChannelId;
use crate::sv1::sv1_server::data::Sv1ServerData;

#[derive(Debug)]
pub struct DownstreamData {
    pub channel_id: Option<u32>,
    pub downstream_id: u32,
    pub extranonce1: Vec<u8>,
    pub extranonce2_len: usize,
    pub version_rolling_mask: Option<HexU32Be>,
    pub version_rolling_min_bit: Option<HexU32Be>,
    pub last_job_version_field: Option<u32>,
    pub authorized_worker_name: String,
    pub user_identity: String,
    pub target: Target,
    pub hashrate: Option<f32>,
    pub cached_set_difficulty: Option<json_rpc::Message>,
    pub cached_notify: Option<json_rpc::Message>,
    pub pending_target: Option<Target>,
    pub pending_hashrate: Option<f32>,
    // Flag to track if SV1 handshake is complete (subscribe + authorize)
    pub sv1_handshake_complete: AtomicBool,
    // Queue of Sv1 handshake messages received while waiting for SV2 channel to open
    pub queued_sv1_handshake_messages: Vec<json_rpc::Message>,
    // Flag to indicate we're processing queued Sv1 handshake message responses
    pub processing_queued_sv1_handshake_responses: AtomicBool,
    // Stores pending shares to be sent to the sv1_server
    pub pending_share: RefCell<Option<SubmitShareWithChannelId>>,
    // Reference to shared sv1_server data for accessing valid_jobs during downstream sv1
    // validation
    pub sv1_server_data: Arc<Mutex<Sv1ServerData>>,
    // Tracks the upstream target for this downstream, used for vardiff target comparison
    pub upstream_target: Option<Target>,
}

impl DownstreamData {
    pub fn new(
        downstream_id: u32,
        target: Target,
        hashrate: Option<f32>,
        sv1_server_data: Arc<Mutex<Sv1ServerData>>,
    ) -> Self {
        DownstreamData {
            channel_id: None,
            downstream_id,
            extranonce1: vec![0; 8],
            extranonce2_len: 4,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            last_job_version_field: None,
            authorized_worker_name: String::new(),
            user_identity: String::new(),
            target,
            hashrate,
            cached_set_difficulty: None,
            cached_notify: None,
            pending_target: None,
            pending_hashrate: None,
            sv1_handshake_complete: AtomicBool::new(false),
            queued_sv1_handshake_messages: Vec::new(),
            processing_queued_sv1_handshake_responses: AtomicBool::new(false),
            pending_share: RefCell::new(None),
            sv1_server_data,
            upstream_target: None,
        }
    }

    pub fn set_pending_target(&mut self, new_target: Target) {
        self.pending_target = Some(new_target);
        debug!("Downstream {}: Set pending target", self.downstream_id);
    }

    pub fn set_pending_hashrate(&mut self, new_hashrate: Option<f32>) {
        self.pending_hashrate = new_hashrate;
        debug!("Downstream {}: Set pending hashrate", self.downstream_id);
    }

    pub fn set_upstream_target(&mut self, upstream_target: Target) {
        self.upstream_target = Some(upstream_target.clone());
        debug!(
            "Downstream {}: Set upstream target to {:?}",
            self.downstream_id, upstream_target
        );
    }
}
