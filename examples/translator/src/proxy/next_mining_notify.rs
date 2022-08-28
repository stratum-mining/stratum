use crate::{
    downstream_sv1,
    error::{Error, ProxyResult},
    upstream_sv2::MiningMessage,
};
use roles_logic_sv2::mining_sv2::SubmitSharesSuccess;
use v1::{json_rpc, methods::Server2Client, server_to_client};

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
    // fn new_mining_notify(self) -> ProxyResult<json_rpc::Message> {
    /// `mining.notify`:  subscription id
    /// extranonce1
    /// extranonce_size2
    pub(crate) fn handle_subscribe_response(&self) -> json_rpc::Message {
        println!("IN NEW_MINING_NOTIFY: {:?}", &self);
        let extra_nonce1 = downstream_sv1::new_extranonce();
        // let extranonce1_str: String = extra_nonce1.try_into().unwrap();
        let extra_nonce2_size = downstream_sv1::new_extranonce2_size();
        let difficulty = downstream_sv1::new_difficulty();
        let difficulty: String = difficulty.try_into().unwrap();
        let set_difficulty_str = format!("[\"mining.set_difficulty\", \"{}\"]", difficulty);
        let subscription_id = downstream_sv1::new_subscription_id();
        let subscription_id: String = subscription_id.try_into().unwrap();
        let notify_str = format!("[\"mining.notify\", \"{}\"]", subscription_id);
        let id = "1".to_string();
        let subscriptions = vec![(set_difficulty_str, notify_str)];

        let subscribe_response = server_to_client::Subscribe {
            id,
            extra_nonce1,
            extra_nonce2_size,
            subscriptions,
        };
        subscribe_response.into()
    }
}
