use async_std::net::{TcpListener, TcpStream};
use std::convert::{TryFrom, TryInto};

use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use std::{env, net::SocketAddr, process::exit, time, time::Duration};
use time::SystemTime;

const ADDR: &str = "127.0.0.1:0";

use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be, MerkleNode, PrevHash},
    ClientStatus, IsClient, IsServer,
};

fn new_extranonce<'a>() -> Extranonce<'a> {
    extranonce_from_hex("08000002")
}

fn extranonce_from_hex<'a>(hex: &str) -> Extranonce<'a> {
    let data = utils::decode_hex(hex).unwrap();
    Extranonce::try_from(data).expect("Failed to convert hex to U256")
}

fn merklenode_from_hex<'a>(hex: &str) -> MerkleNode<'a> {
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

fn prevhash_from_hex<'a>(hex: &str) -> PrevHash<'a> {
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

fn new_extranonce2_size() -> usize {
    4
}

fn new_version_rolling_mask() -> HexU32Be {
    HexU32Be(0xffffffff)
}
fn new_version_rolling_min() -> HexU32Be {
    HexU32Be(0x00000000)
}

struct Server<'a> {
    authorized_names: Vec<String>,
    extranonce1: Extranonce<'a>,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

async fn server_pool_listen(listener: TcpListener) {
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        println!("SERVER - Accepting from: {}", stream.peer_addr().unwrap());
        let server = Server::new(stream).await;
        Arc::new(Mutex::new(server));
    }
}

impl<'a> Server<'a> {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Server<'static>>> {
        let stream = Arc::new(stream);

        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                // println!("{}", message);
                sender_incoming.send(message).await.unwrap();
            }
        });

        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

        let server = Server {
            authorized_names: vec![],
            extranonce1: extranonce_from_hex("00000000"),
            extranonce2_size: 2,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            receiver_incoming,
            sender_outgoing,
        };

        let server = Arc::new(Mutex::new(server));

        let cloned = server.clone();
        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    let incoming = self_.receiver_incoming.try_recv();
                    self_.parse_message(incoming).await;
                    drop(self_);
                    //It's healthy to sleep after giving up the lock so the other thread has a shot
                    //at acquiring it.
                    task::sleep(Duration::from_millis(100)).await;
                };
            }
        });

        let cloned = server.clone();
        task::spawn(async move {
            let mut run_time = Self::get_runtime();

            loop {
                let notify_time = 5;
                if let Some(mut self_) = cloned.try_lock() {
                    let sender = &self_.sender_outgoing.clone();
                    let notify = self_.notify().unwrap();
                    Server::send_message(sender, notify).await;
                    drop(self_);
                    task::sleep(Duration::from_secs(notify_time)).await;
                    //subtract notify_time from run_time
                    run_time -= notify_time as i32;

                    if run_time <= 0 {
                        println!("Test Success - ran for {} seconds", Self::get_runtime());
                        exit(0)
                    }
                };
            }
        });

        server
    }

    fn get_runtime() -> i32 {
        let args: Vec<String> = env::args().collect();
        if args.len() > 1 {
            args[1].parse::<i32>().unwrap()
        } else {
            i32::MAX
        }
    }

    #[allow(clippy::single_match)]
    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("SERVER - message: {}", line);
            let message: Result<json_rpc::Message, _> = serde_json::from_str(&line);
            match message {
                Ok(message) => {
                    match self.handle_message(message) {
                        Ok(response) => {
                            if response.is_some() {
                                Self::send_message(
                                    &self.sender_outgoing,
                                    json_rpc::Message::OkResponse(response.unwrap()),
                                )
                                .await;
                            }
                        }
                        Err(_) => (),
                    };
                }
                Err(_) => (),
            }
        };
    }

    async fn send_message(sender_outgoing: &Sender<String>, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        sender_outgoing.send(msg).await.unwrap();
    }
}

impl<'a> IsServer<'a> for Server<'a> {
    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        self.version_rolling_mask = self
            .version_rolling_mask
            .clone()
            .map_or(Some(new_version_rolling_mask()), Some);
        self.version_rolling_min_bit = self
            .version_rolling_mask
            .clone()
            .map_or(Some(new_version_rolling_min()), Some);
        (
            Some(
                server_to_client::VersionRollingParams::new(
                    self.version_rolling_mask.clone().unwrap(),
                    self.version_rolling_min_bit.clone().unwrap(),
                )
                .unwrap(),
            ),
            Some(false),
        )
    }

    fn handle_subscribe(&self, _request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        vec![]
    }

    fn handle_authorize(&self, _request: &client_to_server::Authorize) -> bool {
        true
    }

    fn handle_submit(&self, _request: &client_to_server::Submit) -> bool {
        true
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    fn is_authorized(&self, _name: &str) -> bool {
        true
    }

    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string())
    }

    /// Set extranonce1 to extranonce1 if provided. If not create a new one and set it.
    fn set_extranonce1(&mut self, extranonce1: Option<Extranonce<'a>>) -> Extranonce<'a> {
        self.extranonce1 = extranonce1.unwrap_or_else(new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> Extranonce<'a> {
        self.extranonce1.clone()
    }

    /// Set extranonce2_size to extranonce2_size if provided. If not create a new one and set it.
    fn set_extranonce2_size(&mut self, extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_size = extra_nonce2_size.unwrap_or_else(new_extranonce2_size);
        self.extranonce2_size
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

    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, Error<'a>> {
        let hex = "ffff";
        Ok(server_to_client::Notify {
            job_id: "ciao".to_string(),
            prev_hash: prevhash_from_hex(hex),
            coin_base1: hex.try_into()?,
            coin_base2: hex.try_into()?,
            merkle_branch: vec![merklenode_from_hex(hex)],
            version: HexU32Be(5667),
            bits: HexU32Be(5678),
            time: HexU32Be(5609),
            clean_jobs: true,
        }
        .into())
    }
}

struct Client<'a> {
    client_id: u32,
    extranonce1: Extranonce<'a>,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    status: ClientStatus,
    last_notify: Option<server_to_client::Notify<'a>>,
    sented_authorize_request: Vec<(u64, String)>, // (id, user_name)
    authorized: Vec<String>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

impl<'a> Client<'static> {
    pub async fn new(client_id: u32, socket: SocketAddr) -> Arc<Mutex<Client<'static>>> {
        let stream = loop {
            task::sleep(Duration::from_secs(1)).await;

            match TcpStream::connect(socket).await {
                Ok(st) => {
                    println!("CLIENT - connected to server at {}", socket);
                    break st;
                }
                Err(_) => {
                    println!("Server not ready... retry");
                    continue;
                }
            }
        };

        let arc_stream = Arc::new(stream);

        let (reader, writer) = (arc_stream.clone(), arc_stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                // println!("{}", message);
                sender_incoming.send(message).await.unwrap();
            }
        });

        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

        let client = Client {
            client_id,
            extranonce1: extranonce_from_hex("00000000"),
            extranonce2_size: 2,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            status: ClientStatus::Init,
            last_notify: None,
            sented_authorize_request: vec![],
            authorized: vec![],
            receiver_incoming,
            sender_outgoing,
        };

        let client = Arc::new(Mutex::new(client));

        let cloned = client.clone();

        task::spawn(async move {
            loop {
                if let Some(mut self_) = cloned.try_lock() {
                    let incoming = self_.receiver_incoming.try_recv();
                    self_.parse_message(incoming).await;
                }
                //It's healthy to sleep after giving up the lock so the other thread has a shot
                //at acquiring it - it also prevents pegging the cpu
                task::sleep(Duration::from_millis(100)).await;
            }
        });

        client
    }

    async fn parse_message(
        &mut self,
        incoming_message: Result<String, async_channel::TryRecvError>,
    ) {
        if let Ok(line) = incoming_message {
            println!("CLIENT {} - message: {}", self.client_id, line);
            let message: json_rpc::Message = serde_json::from_str(&line).unwrap();
            self.handle_message(message).unwrap();
        };
    }

    async fn send_message(sender_outgoing: &Sender<String>, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        sender_outgoing.send(msg).await.unwrap();
    }

    pub async fn send_subscribe(&mut self) {
        loop {
            if let ClientStatus::Configured = self.status {
                break;
            }
        }
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subscribe = self.subscribe(id, None).unwrap();
        Self::send_message(&self.sender_outgoing, subscribe).await;
    }

    //pub async fn restore_subscribe(&mut self) {
    //    let id = time::SystemTime::now()
    //        .duration_since(time::SystemTime::UNIX_EPOCH)
    //        .unwrap()
    //        .as_nanos()
    //        .to_string();
    //    let subscribe = self.subscribe(id, Some(self.extranonce1.clone())).unwrap();
    //    self.send_message(subscribe).await;
    //}

    pub async fn send_authorize(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if let Ok(authorize) = self.authorize(id, "user".to_string(), "user".to_string()) {
            Self::send_message(&self.sender_outgoing, authorize).await;
        }
    }

    pub async fn send_submit(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let extranonce2 = extranonce_from_hex("00");
        let nonce = 78;
        let version_bits = None;
        let submit = self
            .submit(
                id,
                "user".to_string(),
                extranonce2,
                nonce,
                nonce,
                version_bits,
            )
            .unwrap();
        Self::send_message(&self.sender_outgoing, submit).await;
    }

    pub async fn send_configure(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let configure = self.configure(id);
        Self::send_message(&self.sender_outgoing, configure).await;
    }
}

impl<'a> IsClient<'a> for Client<'a> {
    fn handle_set_difficulty(
        &mut self,
        _conf: &mut server_to_client::SetDifficulty,
    ) -> Result<(), Error<'a>> {
        Ok(())
    }

    fn handle_set_extranonce(
        &mut self,
        _conf: &mut server_to_client::SetExtranonce,
    ) -> Result<(), Error<'a>> {
        Ok(())
    }

    fn handle_set_version_mask(
        &mut self,
        _conf: &mut server_to_client::SetVersionMask,
    ) -> Result<(), Error<'a>> {
        Ok(())
    }

    fn handle_notify(&mut self, notify: server_to_client::Notify<'a>) -> Result<(), Error<'a>> {
        self.last_notify = Some(notify);
        Ok(())
    }

    fn handle_configure(
        &mut self,
        _conf: &mut server_to_client::Configure,
    ) -> Result<(), Error<'a>> {
        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        _subscribe: &server_to_client::Subscribe<'a>,
    ) -> Result<(), Error<'a>> {
        Ok(())
    }

    fn set_extranonce1(&mut self, extranonce1: Extranonce<'a>) {
        self.extranonce1 = extranonce1;
    }

    fn extranonce1(&self) -> Extranonce<'a> {
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
                self.sented_authorize_request.push((id, "user".to_string()));
                Ok(client_to_server::Authorize { id, name, password }.into())
            }
        }
    }

    fn last_notify(&self) -> Option<server_to_client::Notify> {
        self.last_notify.clone()
    }

    fn handle_error_message(
        &mut self,
        message: v1::Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>> {
        println!("{:?}", message);
        Ok(None)
    }
}

async fn initialize_client(client: Arc<Mutex<Client<'static>>>) {
    loop {
        let mut client_ = client.lock().await;
        match client_.status {
            ClientStatus::Init => client_.send_configure().await,
            ClientStatus::Configured => client_.send_subscribe().await,
            ClientStatus::Subscribed => {
                client_.send_authorize().await;
                break;
            }
        }
        drop(client_);
        task::sleep(Duration::from_millis(1000)).await;
    }
    task::sleep(Duration::from_millis(2000)).await;
    loop {
        let mut client_ = client.lock().await;
        client_.send_submit().await;
        task::sleep(Duration::from_millis(2000)).await;
    }
}

fn main() {
    //Listen on available port and wait for bind
    let listener = task::block_on(async move {
        let listener = TcpListener::bind(ADDR).await.unwrap();
        println!("Server listening on: {}", listener.local_addr().unwrap());
        listener
    });

    let socket = listener.local_addr().unwrap();

    std::thread::spawn(|| {
        task::spawn(async move {
            server_pool_listen(listener).await;
        });
    });

    task::block_on(async {
        let client = Client::new(80, socket).await;
        initialize_client(client).await;
    });
}

mod utils {
    use std::fmt::Write;

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
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            write!(&mut s, "{:02x}", b).unwrap();
        }
        s
    }

    #[cfg(test)]
    #[test]
    fn test() {
        let test_vec = vec![222, 173, 190, 239];
        let hex_og = "deadbeef";
        let hex_og_w_prefix = "0xdeadbeef";
        // decode hex strings
        let decoded = decode_hex(hex_og).unwrap();
        let decoded_w_prefix = decode_hex(hex_og_w_prefix).unwrap();

        assert_eq!(&decoded, &test_vec, "Hex not decoded correctly");
        assert_eq!(
            &decoded_w_prefix, &test_vec,
            "Hex w/ prefix not decoded correctly"
        );

        // reencode
        let reencoded = encode_hex(&decoded);
        let reencoded_prefix = encode_hex(&decoded);

        assert_eq!(&reencoded, hex_og, "Hex not encoded correctly");
        assert_eq!(
            &reencoded_prefix, &hex_og,
            "Hex w/ prefix not encoded correctly"
        );
    }
}
