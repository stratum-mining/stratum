use crate::{
    error::{Error, ProxyResult},
    upstream_sv2::MiningMessage,
};
#[derive(Clone, Debug)]
pub(crate) struct NextMiningNotify {
    pub(crate) set_new_prev_hash: Option<MiningMessage>,
    pub(crate) new_extended_mining_job: Option<MiningMessage>,
}

impl NextMiningNotify {
    pub(crate) fn new() -> Self {
        NextMiningNotify {
            set_new_prev_hash: None,
            new_extended_mining_job: None,
        }
    }
    // fn new_mining_notify(self) -> json_rpc::Message {
    pub(crate) fn new_mining_notify(&self) {
        let ret = r#"{"params": ["bf", "4d16b6f85af6e2198f44ae2a6de67f78487ae5611b77c6c0440b921e00000000", "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff20020862062f503253482f04b8864e5008", "072f736c7573682f000000000100f2052a010000001976a914d23fcdf86f7e756a64a7a9688ef9903327048ed988ac00000000", [], "00000002", "1c2ac4af", "504e86b9", false], "id": null, "method": "mining.notify"}"#;
    }
}
