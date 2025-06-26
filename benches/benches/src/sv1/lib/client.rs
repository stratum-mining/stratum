//! The defines a sv1 `Client` struct that handles message exchange with the server.
//! It includes methods for initializing the client, parsing messages, and sending various types of
//! messages. It also provides a trait implementation for handling server messages and managing
//! client state.

use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be, MerkleNode, PrevHash},
    ClientStatus, IsClient,
};

#[derive(Debug, Clone)]
pub struct Client {
    client_id: u32,
    extranonce1: Extranonce<'static>,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    pub status: ClientStatus,
    last_notify: Option<server_to_client::Notify<'static>>,
    sented_authorize_request: Vec<(u64, String)>, // (id, user_name)
    authorized: Vec<String>,
}

impl Client {
    pub fn new(client_id: u32) -> Client {
        Client {
            client_id,
            extranonce1: extranonce_from_hex("00000000"),
            extranonce2_size: 4,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            status: ClientStatus::Init,
            last_notify: None,
            sented_authorize_request: vec![],
            authorized: vec![],
        }
    }

    // this is what we want to benchmark
    pub fn parse_message(incoming_message: String) -> json_rpc::Message {
        match serde_json::from_str::<json_rpc::Message>(&incoming_message) {
            Ok(message) => message,
            // no need to handle errors in benchmarks
            Err(_err) => panic!(),
        }
    }

    // also this is what we want to benchmark
    pub fn serialize_message(msg: json_rpc::Message) -> String {
        let json_msg = serde_json::to_string(&msg);
        match json_msg {
            Ok(json_str) => json_str,
            // no need to handle errors in benchmarks
            Err(_err) => panic!(),
        }
    }
}

impl IsClient<'static> for Client {
    fn handle_set_difficulty(
        &mut self,
        _conf: &mut server_to_client::SetDifficulty,
    ) -> Result<(), Error<'static>> {
        Ok(())
    }
    fn handle_set_version_mask(
        &mut self,
        _conf: &mut server_to_client::SetVersionMask,
    ) -> Result<(), Error<'static>> {
        Ok(())
    }

    fn handle_set_extranonce(
        &mut self,
        _conf: &mut server_to_client::SetExtranonce,
    ) -> Result<(), Error<'static>> {
        Ok(())
    }

    fn handle_notify(
        &mut self,
        notify: server_to_client::Notify<'static>,
    ) -> Result<(), Error<'static>> {
        self.last_notify = Some(notify);
        Ok(())
    }

    fn handle_configure(
        &mut self,
        _conf: &mut server_to_client::Configure,
    ) -> Result<(), Error<'static>> {
        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        _subscribe: &server_to_client::Subscribe<'static>,
    ) -> Result<(), Error<'static>> {
        Ok(())
    }

    fn set_extranonce1(&mut self, extranonce1: Extranonce<'static>) {
        self.extranonce1 = extranonce1;
    }

    fn extranonce1(&self) -> Extranonce<'static> {
        self.extranonce1.clone()
    }

    fn set_extranonce2_size(&mut self, extra_nonce2_size: usize) {
        self.extranonce2_size = extra_nonce2_size;
    }

    fn extranonce2_size(&self) -> usize {
        self.extranonce2_size
    }

    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    fn set_version_rolling_min_bit(&mut self, min: Option<HexU32Be>) {
        self.version_rolling_min_bit = min;
    }

    fn set_status(&mut self, status: ClientStatus) {
        self.status = status;
    }

    fn signature(&self) -> String {
        format!("{}", self.client_id)
    }

    fn status(&self) -> ClientStatus {
        self.status
    }

    fn version_rolling_min_bit(&mut self) -> Option<HexU32Be> {
        self.version_rolling_min_bit.clone()
    }

    fn id_is_authorize(&mut self, id: &u64) -> Option<String> {
        let req: Vec<&(u64, String)> = self
            .sented_authorize_request
            .iter()
            .filter(|x| x.0 == *id)
            .collect();
        match req.len() {
            0 => None,
            _ => Some(req[0].1.clone()),
        }
    }

    fn id_is_submit(&mut self, _: &u64) -> bool {
        false
    }

    fn authorize_user_name(&mut self, name: String) {
        self.authorized.push(name)
    }

    fn is_authorized(&self, name: &String) -> bool {
        self.authorized.contains(name)
    }

    fn authorize(
        &mut self,
        id: u64,
        name: String,
        password: String,
    ) -> Result<json_rpc::Message, Error> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.authorize".to_string())),
            _ => {
                self.sented_authorize_request.push((id, name.to_string()));
                Ok(client_to_server::Authorize { id, name, password }.into())
            }
        }
    }

    fn last_notify(&self) -> Option<server_to_client::Notify> {
        self.last_notify.clone()
    }

    fn handle_error_message(
        &mut self,
        _message: v1::Message,
    ) -> Result<Option<json_rpc::Message>, Error<'static>> {
        Ok(None)
    }
}
pub fn extranonce_from_hex(hex: &str) -> Extranonce<'static> {
    let data = utils::decode_hex(hex).unwrap();
    Extranonce::try_from(data).expect("Failed to convert hex to U256")
}
pub fn prevhash_from_hex<'a>(hex: &str) -> PrevHash<'a> {
    let data = utils::decode_hex(hex).unwrap();
    let len = data.len();
    if hex.len() >= 64 {
        // panic if hex is larger than 32 bytes
        PrevHash::try_from(hex).expect("Failed to convert hex to U256")
    } else {
        // prepend hex with zeros so that it is 32 bytes
        let mut new_vec = vec![0_u8; 32 - len];
        new_vec.extend(data.iter());
        PrevHash::try_from(utils::encode_hex(&new_vec).as_str())
            .expect("Failed to convert hex to U256")
    }
}

pub fn merklenode_from_hex<'a>(hex: &str) -> MerkleNode<'a> {
    let data = utils::decode_hex(hex).unwrap();
    let len = data.len();
    if hex.len() >= 64 {
        // panic if hex is larger than 32 bytes
        MerkleNode::try_from(hex).expect("Failed to convert hex to U256")
    } else {
        // prepend hex with zeros so that it is 32 bytes
        let mut new_vec = vec![0_u8; 32 - len];
        new_vec.extend(data.iter());
        MerkleNode::try_from(utils::encode_hex(&new_vec).as_str())
            .expect("Failed to convert hex to U256")
    }
}
pub fn notify(client: &mut Client) {
    client.status = ClientStatus::Subscribed;
    let notify = v1::server_to_client::Notify {
        job_id: "ciao".to_string(),
        prev_hash: prevhash_from_hex("00"),
        coin_base1: "00".try_into().unwrap(),
        coin_base2: "00".try_into().unwrap(),
        merkle_branch: vec![merklenode_from_hex("00")],
        version: HexU32Be(5667),
        bits: HexU32Be(5678),
        time: HexU32Be(5609),
        clean_jobs: true,
    };
    Client::handle_notify(client, notify).unwrap();
}

mod utils {

    pub fn decode_hex(s: &str) -> Result<Vec<u8>, core::num::ParseIntError> {
        let s = match s.strip_prefix("0x") {
            Some(s) => s,
            None => s,
        };
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    pub fn encode_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
