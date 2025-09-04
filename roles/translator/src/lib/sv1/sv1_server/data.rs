use crate::sv1::downstream::downstream::Downstream;
use roles_logic_sv2::{
    mining_sv2::{SetNewPrevHash, Target},
    utils::Id as IdFactory,
    vardiff::classic::VardiffState,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use v1::server_to_client;

#[derive(Debug, Clone)]
pub struct PendingTargetUpdate {
    pub downstream_id: u32,
    pub new_target: Target,
    pub new_hashrate: f32,
}

#[derive(Debug)]
pub struct Sv1ServerData {
    pub downstreams: HashMap<u32, Arc<Downstream>>,
    pub vardiff: HashMap<u32, Arc<RwLock<VardiffState>>>,
    pub prevhash: Option<SetNewPrevHash<'static>>,
    pub downstream_id_factory: IdFactory,
    /// Job storage for aggregated mode - all Sv1 downstreams share the same jobs
    pub aggregated_valid_jobs: Option<Vec<server_to_client::Notify<'static>>>,
    /// Job storage for non-aggregated mode - each Sv1 downstream has its own jobs
    pub non_aggregated_valid_jobs:
        Option<std::collections::HashMap<u32, Vec<server_to_client::Notify<'static>>>>,
    /// Tracks pending target updates that are waiting for SetTarget response from upstream
    pub pending_target_updates: Vec<PendingTargetUpdate>,
    /// The initial target used when opening channels - used when no downstreams remain
    pub initial_target: Option<Target>,
}

impl Sv1ServerData {
    pub fn new(aggregate_channels: bool) -> Self {
        let (aggregated_valid_jobs, non_aggregated_valid_jobs) = if aggregate_channels {
            (Some(Vec::new()), None)
        } else {
            (None, Some(std::collections::HashMap::new()))
        };

        Self {
            downstreams: HashMap::new(),
            vardiff: HashMap::new(),
            prevhash: None,
            downstream_id_factory: IdFactory::new(),
            aggregated_valid_jobs,
            non_aggregated_valid_jobs,
            pending_target_updates: Vec::new(),
            initial_target: None,
        }
    }
}
