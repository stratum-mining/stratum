use crate::sv1::downstream::downstream::Downstream;
use roles_logic_sv2::{
    mining_sv2::SetNewPrevHash, utils::Id as IdFactory, vardiff::classic::VardiffState,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub struct Sv1ServerData {
    pub downstreams: HashMap<u32, Arc<Downstream>>,
    pub vardiff: HashMap<u32, Arc<RwLock<VardiffState>>>,
    pub prevhash: Option<SetNewPrevHash<'static>>,
    pub downstream_id_factory: IdFactory,
}

impl Sv1ServerData {
    pub fn new() -> Self {
        Self {
            downstreams: HashMap::new(),
            vardiff: HashMap::new(),
            prevhash: None,
            downstream_id_factory: IdFactory::new(),
        }
    }
}
