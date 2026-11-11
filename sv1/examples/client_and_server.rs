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
    json_rpc,
    utils::{Extranonce, HexBytes, HexU32Be, MerkleNode, PrevHash, VERSION_ROLLING_MASK},
    Authorize, ClientRequest, Configure, ConfigureResult, Id, IsClient, IsServer, Message, Method,
    Notify, Response, RpcError, SetDifficulty, SetExtranonce, SetVersionMask, Submit,
    SubmitOutcome, Subscribe, SubscribeResult, SuggestDifficulty, VersionRollingResult,
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

#[derive(Default)]
struct Server {
    workers: Vec<String>,
    mask: Option<HexU32Be>,
}

impl IsServer for Server {
    type Error = Error;

    fn handle_configure(
        &mut self,
        _client_id: Option<usize>,
        request: &Configure,
    ) -> Result<ConfigureResult, Error> {
        self.mask = request
            .version_rolling_mask()
            .map(|mask| HexU32Be(mask.0 & VERSION_ROLLING_MASK));
        let min_bit_count = request.version_rolling_min_bit_count().unwrap_or(0);
        Ok(ConfigureResult {
            version_rolling: self
                .mask
                .clone()
                .map(|mask| VersionRollingResult::new(mask, min_bit_count)),
            minimum_difficulty: None,
        })
    }

    fn handle_subscribe(
        &mut self,
        _client_id: Option<usize>,
        _request: &Subscribe,
    ) -> Result<SubscribeResult, Error> {
        Ok(SubscribeResult {
            subscriptions: vec![("mining.notify".to_string(), "1".to_string())],
            extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).expect("4 bytes"),
            extranonce2_size: EXTRANONCE2_SIZE,
        })
    }

    fn handle_authorize(
        &mut self,
        _client_id: Option<usize>,
        request: &Authorize,
    ) -> Result<bool, Error> {
        self.workers.push(request.worker_name.clone());
        Ok(true)
    }

    fn handle_submit(
        &mut self,
        _client_id: Option<usize>,
        _request: &Submit,
    ) -> Result<SubmitOutcome, Error> {
        Ok(SubmitOutcome::Accepted)
    }

    fn handle_extranonce_subscribe(&mut self, _client_id: Option<usize>) -> Result<(), Error> {
        Ok(())
    }

    fn handle_suggest_difficulty(
        &mut self,
        _client_id: Option<usize>,
        _request: &SuggestDifficulty,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn is_authorized(&self, _client_id: Option<usize>, name: &str) -> Result<bool, Error> {
        Ok(self.workers.iter().any(|worker| worker == name))
    }

    fn extranonce2_size(&self, _client_id: Option<usize>) -> Result<usize, Error> {
        Ok(EXTRANONCE2_SIZE)
    }

    fn version_rolling_mask(&self, _client_id: Option<usize>) -> Result<Option<HexU32Be>, Error> {
        Ok(self.mask.clone())
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
        let _ = jobs.send(SetDifficulty { difficulty: 1.0 }.into());
        for id in 0.. {
            if jobs.send(job(id).into()).is_err() {
                break;
            }
            thread::sleep(JOB_INTERVAL);
        }
    });

    let mut server = Server::default();
    for message in incoming {
        println!("SERVER <- {message}");
        let Message::Request(request) = message else {
            continue;
        };
        match ClientRequest::try_from(request) {
            Ok(request) => {
                let response = server
                    .handle_request(Some(0), request)
                    .expect("this server's handlers never fail");
                if let Some(response) = response {
                    let _ = outgoing.send(Message::Response(response));
                }
            }
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
    fn send<P>(&mut self, request: fn(Id, P) -> ClientRequest, params: P) {
        let id = Id::from(self.next_id);
        self.next_id += 1;
        let message = Message::from(request(id.clone(), params));
        if let Message::Request(request) = &message {
            self.pending.insert(id, request.method.clone());
        }
        let _ = self.outgoing.send(message);
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
            version_bits: self.mask.clone().map(|mask| HexU32Be(0x2000 & mask.0)),
        };
        self.send(ClientRequest::Submit, submit);
    }
}

impl IsClient for Client {
    type Error = Error;

    fn request_method(&self, _server_id: Option<usize>, id: &Id) -> Result<Option<Method>, Error> {
        Ok(self.pending.get(id).cloned())
    }

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
        id: &Id,
        result: ConfigureResult,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        self.mask = result
            .version_rolling
            .filter(|version_rolling| version_rolling.enabled)
            .map(|version_rolling| version_rolling.mask);
        Ok(())
    }

    fn handle_subscribe_result(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        result: SubscribeResult,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        self.extranonce2_size = Some(result.extranonce2_size);
        Ok(())
    }

    fn handle_authorize_result(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        authorized: bool,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        self.authorized = authorized;
        Ok(())
    }

    fn handle_submit_result(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        accepted: bool,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        if accepted {
            self.accepted += 1;
        }
        Ok(())
    }

    fn handle_extranonce_subscribe_result(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        _acknowledged: bool,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        Ok(())
    }

    fn handle_error_response(
        &mut self,
        _server_id: Option<usize>,
        id: &Id,
        method: Method,
        error: RpcError,
    ) -> Result<(), Error> {
        self.pending.remove(id);
        eprintln!(
            "CLIENT - {method} {id} rejected: {} {}",
            error.code, error.message
        );
        Ok(())
    }

    fn handle_unknown_response(
        &mut self,
        _server_id: Option<usize>,
        response: Response,
    ) -> Result<(), Error> {
        eprintln!("CLIENT - unexpected response {response}");
        Ok(())
    }

    fn handle_unknown_request(
        &mut self,
        _server_id: Option<usize>,
        request: json_rpc::Request,
    ) -> Result<(), Error> {
        eprintln!("CLIENT - unsupported request {request}");
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
        ClientRequest::Configure,
        Configure::with_version_rolling(Some(HexU32Be(0xffff_ffff)), Some(2)),
    );
    client.send(
        ClientRequest::Subscribe,
        Subscribe {
            agent: "sv1-example/1.0".to_string(),
            subscription_id: None,
        },
    );
    client.send(
        ClientRequest::Authorize,
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
                if let Err(e) = client.handle_message(None, message) {
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
