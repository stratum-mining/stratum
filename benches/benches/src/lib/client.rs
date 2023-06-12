use async_std::net::TcpStream;
use std::{
    net::SocketAddr,
    time,
    time::Duration,
};
use time::SystemTime;
use async_channel::{bounded, Receiver, Sender};
use async_std::{
    io::BufReader,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use v1::{
    client_to_server,
    error::Error,
    json_rpc,
    server_to_client,
    utils::{Extranonce, HexU32Be},
    ClientStatus, IsClient
};

pub struct Client<'a> {
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
            status: ClientStatus::Subscribed,
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
    fn handle_notify(&mut self, notify: server_to_client::Notify<'a>) -> Result<(), Error<'a>> {
        self.last_notify = Some(notify);
        Ok(())
    }

    fn handle_configure(&self, _conf: &mut server_to_client::Configure) -> Result<(), Error<'a>> {
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

    fn extranonce2_size(&self) -> usize

 {
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
   fn extranonce_from_hex<'a>(hex: &str) -> Extranonce<'a> {
    let data = utils::decode_hex(hex).unwrap();
    Extranonce::try_from(data).expect("Failed to convert hex to U256")
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

}
pub async fn initialize_client(client: Arc<Mutex<Client<'static>>>) {
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
    for _ in 0..2 {
        let mut client_ = client.lock().await;
        client_.send_submit().await;
        task::sleep(Duration::from_millis(2000)).await;
    }
}
