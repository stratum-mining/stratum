//! An SV1 server and client talking over a local TCP connection for 30 seconds.
//!
//! The server sends a job every 5 seconds. The client configures version rolling, subscribes and
//! authorizes, then submits a share for the latest job every 2 seconds. It exits with an error
//! if no share was accepted.

use std::{
    collections::HashMap,
    convert::TryFrom,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process::exit,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use sv1_api::{
    error::Error,
    utils::{Extranonce, HexBytes, HexU32Be, MerkleNode, PrevHash},
    Authorize, ClientToServer, Configure, ConfigureResult, Id, IsClient, IsServer, Message, Method,
    Notify, Request, Response, RpcError, ServerToClient, SetDifficulty, SetExtranonce,
    SetVersionMask, Submit, SubmitError, SubmitOutcome, Subscribe, SubscribeResult,
    SuggestDifficulty, VersionRollingResult,
};

const RUN_FOR: Duration = Duration::from_secs(30);
const JOB_INTERVAL: Duration = Duration::from_secs(5);
const SUBMIT_INTERVAL: Duration = Duration::from_secs(2);
const EXTRANONCE2_SIZE: usize = 4;
const WORKER: &str = "user";

/// Decodes newline-delimited messages from `stream` into a channel, and writes the messages sent
/// on the returned sender.
fn connect(stream: TcpStream) -> (mpsc::Receiver<Message>, mpsc::Sender<Message>) {
    let (incoming_tx, incoming) = mpsc::channel();
    let reader = BufReader::new(stream.try_clone().expect("failed to clone the stream"));
    thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            match Message::decode(&line) {
                Ok(message) => {
                    if incoming_tx.send(message).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("dropping line: {e}"),
            }
        }
    });

    let (outgoing, outgoing_rx) = mpsc::channel::<Message>();
    let mut writer = stream;
    thread::spawn(move || {
        for message in outgoing_rx {
            let line = format!(
                "{}\n",
                serde_json::to_string(&message).expect("serializable")
            );
            if writer.write_all(line.as_bytes()).is_err() {
                break;
            }
        }
    });
    (incoming, outgoing)
}

struct Server {
    outgoing: mpsc::Sender<Message>,
    workers: Vec<String>,
    mask: Option<HexU32Be>,
}

impl Server {
    fn reply(&self, response: impl Into<Message>) {
        let _ = self.outgoing.send(response.into());
    }
}

impl IsServer for Server {
    type Error = Error;

    fn handle_configure(
        &mut self,
        _client_id: Option<usize>,
        id: Id,
        request: Configure,
    ) -> Result<(), Error> {
        self.mask = request.version_rolling_mask();
        let result = ConfigureResult {
            version_rolling: self.mask.map(VersionRollingResult::new),
            minimum_difficulty: request.minimum_difficulty().map(|_| false),
            subscribe_extranonce: request.supports_extranonce().then_some(true),
            info: request.info().map(|_| true),
            unsupported: request.unknown_extensions().map(String::from).collect(),
        };
        self.reply(Response::success(id, result));
        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        _client_id: Option<usize>,
        id: Id,
        _request: Subscribe,
    ) -> Result<(), Error> {
        let result = SubscribeResult {
            subscriptions: vec![("mining.notify".to_string(), "1".to_string())],
            extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).expect("4 bytes"),
            extranonce2_size: EXTRANONCE2_SIZE,
        };
        self.reply(Response::success(id, result));
        Ok(())
    }

    fn handle_authorize(
        &mut self,
        _client_id: Option<usize>,
        id: Id,
        request: Authorize,
    ) -> Result<(), Error> {
        self.workers.push(request.worker_name);
        self.reply(Response::success(id, true));
        Ok(())
    }

    /// Accepts every share from an authorized worker whose extranonce2 has the assigned size and
    /// whose version bits, if any, the negotiated mask allows.
    fn handle_submit(
        &mut self,
        _client_id: Option<usize>,
        id: Id,
        request: Submit,
    ) -> Result<(), Error> {
        let version_bits_allowed = match (&request.version_bits, self.mask) {
            (None, _) => true,
            (Some(bits), Some(mask)) => mask.check_mask(bits),
            (Some(_), None) => false,
        };
        let outcome = if !self.workers.contains(&request.worker_name) {
            SubmitOutcome::Rejected(SubmitError::UnauthorizedWorker)
        } else if request.extranonce2.len() != EXTRANONCE2_SIZE || !version_bits_allowed {
            SubmitOutcome::Rejected(SubmitError::Other)
        } else {
            SubmitOutcome::Accepted
        };
        self.reply(outcome.into_response(id));
        Ok(())
    }

    fn handle_extranonce_subscribe(
        &mut self,
        _client_id: Option<usize>,
        id: Id,
    ) -> Result<(), Error> {
        self.reply(Response::success(id, true));
        Ok(())
    }

    fn handle_suggest_difficulty(
        &mut self,
        _client_id: Option<usize>,
        _id: Id,
        _request: SuggestDifficulty,
    ) -> Result<(), Error> {
        Ok(())
    }
}

fn job(id: u32) -> Notify {
    Notify {
        job_id: id.to_string(),
        prev_hash: PrevHash::try_from("00".repeat(32).as_str()).expect("32 bytes"),
        coinbase1: HexBytes::from(vec![1, 0, 0, 0]),
        coinbase2: HexBytes::from(vec![0xff; 4]),
        merkle_branch: vec![MerkleNode::try_from(vec![0; 32]).expect("32 bytes")],
        version: HexU32Be(0x2000_0000),
        nbits: HexU32Be(0x1705_ae3a),
        ntime: HexU32Be(0x6470_25b5),
        clean_jobs: true,
    }
}

fn serve(stream: TcpStream) {
    let (incoming, outgoing) = connect(stream);
    let jobs = outgoing.clone();
    thread::spawn(move || {
        let _ = jobs.send(SetDifficulty::new(1.0).expect("positive").into());
        for id in 0.. {
            if jobs.send(job(id).into()).is_err() {
                break;
            }
            thread::sleep(JOB_INTERVAL);
        }
    });

    let mut server = Server {
        outgoing,
        workers: vec![],
        mask: None,
    };
    for message in incoming {
        println!("SERVER <- {message}");
        let Message::Request(request) = message else {
            continue;
        };
        match ClientToServer::try_from(request) {
            Ok(request) => server
                .handle_request(Some(0), request)
                .expect("this server's handlers never fail"),
            Err(e) => eprintln!("SERVER - {e}"),
        }
    }
}

struct Client {
    outgoing: mpsc::Sender<Message>,
    next_id: u64,
    pending: HashMap<Id, Method>,
    extranonce2_size: Option<usize>,
    mask: Option<HexU32Be>,
    authorized: bool,
    job_id: Option<String>,
    accepted: usize,
}

impl Client {
    fn new(outgoing: mpsc::Sender<Message>) -> Self {
        Client {
            outgoing,
            next_id: 1,
            pending: HashMap::new(),
            extranonce2_size: None,
            mask: None,
            authorized: false,
            job_id: None,
            accepted: 0,
        }
    }

    /// Records the request's method so its response can be routed, then sends it.
    fn send<P>(&mut self, request: fn(Request<P>) -> ClientToServer, params: P) {
        let id = Id::from(self.next_id);
        self.next_id += 1;
        let request = request(Request {
            id: id.clone(),
            params,
        });
        self.pending.insert(id, request.method());
        let _ = self.outgoing.send(request.into());
    }

    fn submit(&mut self) {
        let (Some(size), Some(job_id), true) =
            (self.extranonce2_size, self.job_id.clone(), self.authorized)
        else {
            return;
        };
        let submit = Submit {
            worker_name: WORKER.to_string(),
            job_id,
            extranonce2: Extranonce::try_from(vec![0; size]).expect("at most 32 bytes"),
            ntime: HexU32Be(0x6470_25b5),
            nonce: HexU32Be(78),
            version_bits: self.mask.map(|mask| HexU32Be(0x2000 & mask.0)),
        };
        self.send(ClientToServer::Submit, submit);
    }
}

impl Client {
    /// Types a message from the server and passes it to its handler. A reply does not name the
    /// method it answers, so it is typed by the method of the request sent with its ID.
    fn receive(&mut self, message: Message) -> Result<(), Error> {
        let message = match message {
            Message::Request(request) => ServerToClient::try_from(request)?,
            Message::Response(response) => {
                let method = response.id.as_ref().and_then(|id| self.pending.remove(id));
                let typed = match &method {
                    Some(method) => ServerToClient::from_response(&response, method)?,
                    None => None,
                };
                let Some(typed) = typed else {
                    eprintln!("CLIENT - unexpected response {response}");
                    return Ok(());
                };
                typed
            }
        };
        self.handle_message(None, message)
    }
}

impl IsClient for Client {
    type Error = Error;

    fn handle_notify(&mut self, _server_id: Option<usize>, notify: Notify) -> Result<(), Error> {
        self.job_id = Some(notify.job_id);
        Ok(())
    }

    fn handle_set_difficulty(
        &mut self,
        _server_id: Option<usize>,
        _set: SetDifficulty,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn handle_set_extranonce(
        &mut self,
        _server_id: Option<usize>,
        set: SetExtranonce,
    ) -> Result<(), Error> {
        self.extranonce2_size = Some(set.extranonce2_size);
        Ok(())
    }

    fn handle_set_version_mask(
        &mut self,
        _server_id: Option<usize>,
        set: SetVersionMask,
    ) -> Result<(), Error> {
        self.mask = Some(set.mask);
        Ok(())
    }

    fn handle_configure_result(
        &mut self,
        _server_id: Option<usize>,
        _id: &Id,
        result: ConfigureResult,
    ) -> Result<(), Error> {
        self.mask = result
            .version_rolling
            .filter(|version_rolling| version_rolling.enabled)
            .map(|version_rolling| version_rolling.mask);
        Ok(())
    }

    fn handle_subscribe_result(
        &mut self,
        _server_id: Option<usize>,
        _id: &Id,
        result: SubscribeResult,
    ) -> Result<(), Error> {
        self.extranonce2_size = Some(result.extranonce2_size);
        Ok(())
    }

    fn handle_authorize_result(
        &mut self,
        _server_id: Option<usize>,
        _id: &Id,
        authorized: bool,
    ) -> Result<(), Error> {
        self.authorized = authorized;
        Ok(())
    }

    fn handle_submit_result(
        &mut self,
        _server_id: Option<usize>,
        _id: &Id,
        accepted: bool,
    ) -> Result<(), Error> {
        if accepted {
            self.accepted += 1;
        }
        Ok(())
    }

    fn handle_extranonce_subscribe_result(
        &mut self,
        _server_id: Option<usize>,
        _id: &Id,
        _acknowledged: bool,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn handle_error_response(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        method: Method,
        error: RpcError,
    ) -> Result<(), Error> {
        eprintln!(
            "CLIENT - {method} {id} rejected: {} {}",
            error.code, error.message
        );
        Ok(())
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    let address = listener.local_addr().expect("bound address");
    println!("Server listening on: {address}");
    thread::spawn(move || {
        for stream in listener.incoming().map_while(Result::ok) {
            thread::spawn(move || serve(stream));
        }
    });

    let stream = TcpStream::connect(address).expect("failed to connect");
    let (incoming, outgoing) = connect(stream);
    let mut client = Client::new(outgoing);
    client.send(
        ClientToServer::Configure,
        Configure::with_version_rolling(Some(HexU32Be(0xffff_ffff)), Some(2)),
    );
    client.send(
        ClientToServer::Subscribe,
        Subscribe {
            agent: "sv1-example/1.0".to_string(),
            subscription_id: None,
        },
    );
    client.send(
        ClientToServer::Authorize,
        Authorize {
            worker_name: WORKER.to_string(),
            password: "x".to_string(),
        },
    );

    let deadline = Instant::now() + RUN_FOR;
    let mut next_submit = Instant::now() + SUBMIT_INTERVAL;
    while Instant::now() < deadline {
        match incoming.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => {
                println!("CLIENT <- {message}");
                if let Err(e) = client.receive(message) {
                    eprintln!("CLIENT - {e}");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("CLIENT - connection closed");
                exit(1);
            }
        }
        if Instant::now() >= next_submit {
            client.submit();
            next_submit += SUBMIT_INTERVAL;
        }
    }

    println!(
        "Ran for {} seconds; {} shares accepted",
        RUN_FOR.as_secs(),
        client.accepted
    );
    exit(if client.accepted > 0 { 0 } else { 1 });
}
