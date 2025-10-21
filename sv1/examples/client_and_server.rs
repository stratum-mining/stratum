use std::{
    convert::{TryFrom, TryInto},
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::exit,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

const ADDR: &str = "127.0.0.1:0";
const TEST_DURATION: i32 = 30;

type Receiver<T> = mpsc::Receiver<T>;
type Sender<T> = mpsc::Sender<T>;

use sv1_api::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be, MerkleNode, PrevHash},
    ClientStatus, IsClient, IsServer, Message,
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

fn server_pool_listen(listener: TcpListener) {
    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("SERVER - Accepting from: {addr}");
                let server = Server::new(stream);
                let _ = Arc::new(Mutex::new(server));
            }
            Err(e) => {
                eprintln!("SERVER - Accept error: {e}");
                break;
            }
        }
    }
}

impl Server<'_> {
    pub fn new(stream: TcpStream) -> Arc<Mutex<Server<'static>>> {
        let (sender_incoming, receiver_incoming) = mpsc::channel::<String>();
        let (sender_outgoing, receiver_outgoing) = mpsc::channel::<String>();

        let reader_stream = stream.try_clone().expect("Failed to clone stream (read)");
        let mut writer_stream = stream;

        // read thread
        thread::spawn(move || {
            let reader = BufReader::new(reader_stream);
            for line in reader.lines().map_while(Result::ok) {
                if sender_incoming.send(line).is_err() {
                    break;
                }
            }
        });

        thread::spawn(move || {
            for msg in receiver_outgoing {
                let _ = writer_stream.write_all(msg.as_bytes());
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

        let server_arc = Arc::new(Mutex::new(server));

        {
            let cloned = Arc::clone(&server_arc);
            thread::spawn(move || loop {
                if let Ok(mut self_) = cloned.try_lock() {
                    if let Ok(line) = self_.receiver_incoming.try_recv() {
                        println!("SERVER - message: {line}");
                        let message: Result<json_rpc::Message, _> = serde_json::from_str(&line);
                        if let Ok(message) = message {
                            if let Ok(Some(resp)) = self_.handle_message(message) {
                                Self::send_message(
                                    &self_.sender_outgoing,
                                    json_rpc::Message::OkResponse(resp),
                                );
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            });
        }

        {
            let cloned = Arc::clone(&server_arc);
            thread::spawn(move || {
                let mut run_time = TEST_DURATION;
                loop {
                    let notify_time = 5;
                    if let Ok(mut self_) = cloned.try_lock() {
                        let sender = self_.sender_outgoing.clone();
                        if let Ok(notify_msg) = self_.notify() {
                            Server::send_message(&sender, notify_msg);
                        }
                    }
                    thread::sleep(Duration::from_secs(notify_time));
                    run_time -= notify_time as i32;

                    if run_time <= 0 {
                        println!("Test Success - ran for {TEST_DURATION} seconds");
                        exit(0)
                    }
                }
            });
        }

        server_arc
    }

    fn handle_message(
        &mut self,
        _message: json_rpc::Message,
    ) -> Result<Option<json_rpc::Response>, Error<'static>> {
        Ok(None)
    }

    fn send_message(sender_outgoing: &Sender<String>, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        let _ = sender_outgoing.send(msg);
    }
}

impl<'a> IsServer<'a> for Server<'a> {
    fn handle_configure(
        &mut self,
        _request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        self.version_rolling_mask
            .get_or_insert_with(new_version_rolling_mask);
        self.version_rolling_min_bit
            .get_or_insert_with(new_version_rolling_min);

        let mask = self.version_rolling_mask.as_ref().unwrap().clone();
        let min_bit = self.version_rolling_min_bit.as_ref().unwrap().clone();

        (
            Some(server_to_client::VersionRollingParams::new(mask, min_bit).unwrap()),
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

impl Client<'static> {
    pub fn new(client_id: u32, socket: SocketAddr) -> Arc<Mutex<Client<'static>>> {
        loop {
            thread::sleep(Duration::from_secs(1));
            match TcpStream::connect(socket) {
                Ok(st) => {
                    println!("CLIENT - connected to server at {socket}");
                    let (sender_incoming, receiver_incoming) = mpsc::channel::<String>();
                    let (sender_outgoing, receiver_outgoing) = mpsc::channel::<String>();

                    let reader_stream = st.try_clone().unwrap();
                    let mut writer_stream = st;

                    thread::spawn(move || {
                        let reader = BufReader::new(reader_stream);
                        for line in reader.lines().map_while(Result::ok) {
                            if sender_incoming.send(line).is_err() {
                                break;
                            }
                        }
                    });

                    thread::spawn(move || {
                        for msg in receiver_outgoing {
                            let _ = writer_stream.write_all(msg.as_bytes());
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

                    let arc_client = Arc::new(Mutex::new(client));

                    {
                        let cloned = Arc::clone(&arc_client);
                        thread::spawn(move || loop {
                            if let Ok(mut this) = cloned.try_lock() {
                                if let Ok(line) = this.receiver_incoming.try_recv() {
                                    println!("CLIENT {} - message: {}", this.client_id, line);
                                    if let Ok(msg) =
                                        serde_json::from_str::<json_rpc::Message>(&line)
                                    {
                                        this.handle_message(msg).ok();
                                    }
                                }
                            }
                            thread::sleep(Duration::from_millis(100));
                        });
                    }

                    return arc_client;
                }
                Err(_) => {
                    println!("Server not ready... retry");
                    continue;
                }
            }
        }
    }

    fn send_message(sender_outgoing: &Sender<String>, msg: json_rpc::Message) {
        let s = format!("{}\n", serde_json::to_string(&msg).unwrap());
        let _ = sender_outgoing.send(s);
    }

    pub fn send_subscribe(&mut self) {
        while let ClientStatus::Init = self.status {
            thread::sleep(Duration::from_millis(100));
        }
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if let Ok(subscribe) = self.subscribe(id, None) {
            Self::send_message(&self.sender_outgoing, subscribe);
        }
    }

    pub fn send_authorize(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if let Ok(authorize) = self.authorize(id, "user".to_string(), "user".to_string()) {
            Self::send_message(&self.sender_outgoing, authorize);
        }
    }

    pub fn send_submit(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let extranonce2 = extranonce_from_hex("00");
        let nonce = 78;
        let version_bits = None;
        if let Ok(submit) = self.submit(
            id,
            "user".to_string(),
            extranonce2,
            nonce,
            nonce,
            version_bits,
        ) {
            Self::send_message(&self.sender_outgoing, submit);
        }
    }

    pub fn send_configure(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let configure = self.configure(id);
        Self::send_message(&self.sender_outgoing, configure);
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
        message: Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>> {
        println!("{message:?}");
        Ok(None)
    }
}

fn initialize_client(client: Arc<Mutex<Client<'static>>>) {
    loop {
        {
            let mut client_ = client.lock().unwrap();
            match client_.status {
                ClientStatus::Init => client_.send_configure(),
                ClientStatus::Configured => client_.send_subscribe(),
                ClientStatus::Subscribed => {
                    client_.send_authorize();
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }

    thread::sleep(Duration::from_millis(2000));
    loop {
        {
            let mut client_ = client.lock().unwrap();
            client_.send_submit();
        }
        thread::sleep(Duration::from_millis(2000));
    }
}

fn main() {
    let listener = TcpListener::bind(ADDR).unwrap();
    println!("Server listening on: {}", listener.local_addr().unwrap());
    let socket = listener.local_addr().unwrap();

    thread::spawn(move || {
        server_pool_listen(listener);
    });

    let client = Client::new(80, socket);
    initialize_client(client);
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
            write!(&mut s, "{b:02x}").unwrap();
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
