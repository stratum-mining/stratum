use async_std::net::{TcpListener, TcpStream};
use std::convert::TryInto;

use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use std::{env, net::SocketAddr, process::exit, thread::sleep, time, time::Duration};
use time::SystemTime;

const ADDR: &str = "127.0.0.1:0";

use v1::{
    client_to_server,
    error::Error,
    json_rpc, server_to_client,
    utils::{self, HexBytes, HexU32Be},
    ClientStatus, IsClient, IsServer,
};

fn new_extranonce() -> HexBytes {
    "08000002".try_into().unwrap()
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

struct Server {
    authorized_names: Vec<String>,
    extranonce1: HexBytes,
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
        println!(
            "{:?}-SERVER - Accepting from: {}",
            chrono::offset::Local::now(),
            stream.peer_addr().unwrap()
        );
        let server = Server::new(stream).await;
        Arc::new(Mutex::new(server));
    }
}

impl Server {
    pub async fn new(stream: TcpStream) -> Arc<Mutex<Self>> {
        let stream = Arc::new(stream);

        let (reader, writer) = (stream.clone(), stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                println!(
                    "{:?}-Server sender_incoming SENDING message - {}",
                    chrono::offset::Local::now(),
                    message
                );
                sender_incoming.send(message).await.unwrap();
            }
        });

        task::spawn(async move {
            loop {
                let message: String = receiver_outgoing.recv().await.unwrap();
                println!("server - receiver_outgoing writing message - {}", message);

                (&*writer).write_all(message.as_bytes()).await.unwrap();
            }
        });

        let server = Server {
            authorized_names: vec![],
            extranonce1: "00000000".try_into().unwrap(),
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
                };
            }
        });

        let cloned = server.clone();
        task::spawn(async move {
            let mut run_time = Self::get_runtime();
            println!(
                "{}-Starting notify thread loop",
                chrono::offset::Local::now()
            );
            loop {
                let notify_time = 5;
                if let Some(mut self_) = cloned.try_lock() {
                    println!("{}-Sending notify...", chrono::offset::Local::now());

                    self_.send_notify().await;
                    drop(self_);
                    sleep(Duration::from_secs(notify_time));
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
            println!(
                "{:?}-SERVER - message: {}",
                chrono::offset::Local::now(),
                line
            );
            let message: Result<json_rpc::Message, _> = serde_json::from_str(&line);
            match message {
                Ok(message) => {
                    match self.handle_message(message) {
                        Ok(response) => {
                            if response.is_some() {
                                self.send_message(json_rpc::Message::OkResponse(response.unwrap()))
                                    .await;
                            }
                        }
                        Err(_) => (println!("Error parsing message1")),
                    };
                }
                Err(_) => (println!("Error parsing message2")),
            }
        };
    }

    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        self.sender_outgoing.send(msg).await.unwrap();
    }

    async fn send_notify(&mut self) {
        let notify = self.notify().unwrap();
        self.send_message(notify).await;
    }
}

impl IsServer for Server {
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
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap(),
                self.version_rolling_min_bit.clone().unwrap(),
            )),
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
    fn set_extranonce1(&mut self, extranonce1: Option<HexBytes>) -> HexBytes {
        self.extranonce1 = extranonce1.unwrap_or_else(new_extranonce);
        self.extranonce1.clone()
    }

    fn extranonce1(&self) -> HexBytes {
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

    fn notify(&mut self) -> Result<json_rpc::Message, ()> {
        server_to_client::Notify {
            job_id: "ciao".to_string(),
            prev_hash: utils::PrevHash(vec![3_u8, 4, 5, 6]),
            coin_base1: "ffff".try_into().unwrap(),
            coin_base2: "ffff".try_into().unwrap(),
            merkle_branch: vec!["fff".try_into().unwrap()],
            version: HexU32Be(5667),
            bits: HexU32Be(5678),
            time: HexU32Be(5609),
            clean_jobs: true,
        }
        .try_into()
    }
}

struct Client {
    client_id: u32,
    extranonce1: HexBytes,
    extranonce2_size: usize,
    version_rolling_mask: Option<HexU32Be>,
    version_rolling_min_bit: Option<HexU32Be>,
    status: ClientStatus,
    last_notify: Option<server_to_client::Notify>,
    sented_authorize_request: Vec<(String, String)>, // (id, user_name)
    authorized: Vec<String>,
    receiver_incoming: Receiver<String>,
    sender_outgoing: Sender<String>,
}

impl Client {
    pub async fn new(client_id: u32, socket: SocketAddr) -> Arc<Mutex<Self>> {
        let stream = loop {
            sleep(Duration::from_secs(1));

            match TcpStream::connect(socket).await {
                Ok(st) => {
                    println!(
                        "{:?}-CLIENT - connected to server at {}",
                        chrono::offset::Local::now(),
                        socket
                    );
                    break st;
                }
                Err(_) => {
                    println!("Server not ready... retry");
                    continue;
                }
            }
        };

        let arc_stream = std::sync::Arc::new(stream);

        let (reader, writer) = (arc_stream.clone(), arc_stream);

        let (sender_incoming, receiver_incoming) = bounded(10);
        let (sender_outgoing, receiver_outgoing) = bounded(10);

        task::spawn(async move {
            let mut messages = BufReader::new(&*reader).lines();
            while let Some(message) = messages.next().await {
                let message = message.unwrap();
                println!("{}", message);
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
            extranonce1: "00000000".try_into().unwrap(),
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

    async fn send_message(&mut self, msg: json_rpc::Message) {
        let msg = format!("{}\n", serde_json::to_string(&msg).unwrap());
        self.sender_outgoing.send(msg).await.unwrap();
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
            .as_nanos()
            .to_string();
        let subscribe = self.subscribe(id, None).unwrap();
        self.send_message(subscribe).await;
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
            .as_nanos()
            .to_string();
        let authorize = self
            .authorize(id.clone(), "user".to_string(), "user".to_string())
            .unwrap();
        self.sented_authorize_request.push((id, "user".to_string()));
        self.send_message(authorize).await;
    }

    pub async fn send_submit(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let extranonce2 = "00".try_into().unwrap();
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
        self.send_message(submit).await;
    }

    pub async fn send_configure(&mut self) {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let configure = self.configure(id);
        self.send_message(configure).await;
    }
}

impl IsClient for Client {
    fn handle_notify(&mut self, notify: server_to_client::Notify) -> Result<(), Error> {
        self.last_notify = Some(notify);
        Ok(())
    }

    fn handle_configure(&self, _conf: &mut server_to_client::Configure) -> Result<(), Error> {
        Ok(())
    }

    fn handle_subscribe(&mut self, _subscribe: &server_to_client::Subscribe) -> Result<(), Error> {
        Ok(())
    }

    fn set_extranonce1(&mut self, extranonce1: HexBytes) {
        self.extranonce1 = extranonce1;
    }

    fn extranonce1(&self) -> HexBytes {
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

    fn id_is_authorize(&mut self, id: &str) -> Option<String> {
        let req: Vec<&(String, String)> = self
            .sented_authorize_request
            .iter()
            .filter(|x| x.0 == id)
            .collect();
        match req.len() {
            0 => None,
            _ => Some(req[0].1.clone()),
        }
    }

    fn id_is_submit(&mut self, _: &str) -> bool {
        false
    }

    fn authorize_user_name(&mut self, name: String) {
        self.authorized.push(name)
    }

    fn is_authorized(&self, name: &String) -> bool {
        self.authorized.contains(name)
    }

    fn last_notify(&self) -> Option<server_to_client::Notify> {
        self.last_notify.clone()
    }

    fn handle_error_message(
        &mut self,
        message: v1::Message,
    ) -> Result<Option<json_rpc::Response>, Error> {
        println!("{:?}", message);
        Ok(None)
    }
}

async fn initialize_client(client: Arc<Mutex<Client>>) {
    loop {
        println!("{:?} Initialized client", chrono::offset::Local::now());
        let mut client_ = client.lock().await;
        println!("Got client lock");

        match client_.status {
            ClientStatus::Init => {
                println!("{:?} - Client status: init", chrono::offset::Local::now());

                client_.send_configure().await
            }
            ClientStatus::Configured => {
                println!("{:?} - Client status: configured", chrono::offset::Local::now());

                client_.send_subscribe().await
            }
            ClientStatus::Subscribed => {
                println!("{:?} - Client status: subscribed", chrono::offset::Local::now());

                client_.send_authorize().await;
                break;
            }
        }
        drop(client_);
        task::sleep(Duration::from_millis(100)).await;
    }
    task::sleep(Duration::from_millis(2000)).await;
    loop {
        let mut client_ = client.lock().await;
        client_.send_submit().await;
        task::sleep(Duration::from_millis(2000)).await;
    }
}

fn main() {
    // exit application after 50 seconds
    task::spawn(async {
        sleep(Duration::from_secs(50));
        println!("Exiting application");
        exit(0)
        });


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

    //Give the server a full sec to get setup
    sleep(Duration::from_millis(1000));
    println!("slept for a sec - now starting up client");
    task::block_on(async {
        let client = Client::new(80, socket).await;
        initialize_client(client).await;
    });
}
