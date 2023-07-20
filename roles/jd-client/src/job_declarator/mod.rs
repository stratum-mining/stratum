pub mod message_handler;
use async_channel::{Receiver, Sender};
use binary_sv2::{Seq0255, U256};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::SendTo_,
    job_declaration_sv2::AllocateMiningJobTokenSuccess,
    parsers::{JobDeclaration, PoolMessages},
    template_distribution_sv2::SetNewPrevHash,
    utils::Mutex,
};
use std::{collections::HashMap, convert::TryInto, str::FromStr};
use tokio::task::AbortHandle;
use tracing::info;

use async_recursion::async_recursion;
use codec_sv2::Frame;
use nohash_hasher::BuildNoHashHasher;
use roles_logic_sv2::{
    handlers::job_declaration::ParseServerJobDeclarationMessages,
    job_declaration_sv2::{AllocateMiningJobToken, DeclareMiningJob},
    template_distribution_sv2::NewTemplate,
    utils::Id,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

use crate::{proxy_config::ProxyConfig, upstream_sv2::Upstream};

pub struct JobDeclarator {
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
    sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    allocated_tokens: Vec<AllocateMiningJobTokenSuccess<'static>>,
    req_ids: Id,
    min_extranonce_size: u16,
    // (Sented DeclareMiningJob, is future, template id, merkle path)
    last_declare_mining_job_sent: Vec<(
        DeclareMiningJob<'static>,
        bool,
        u64,
        Seq0255<'static, U256<'static>>,
    )>,
    last_set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    future_jobs: HashMap<
        u64,
        (DeclareMiningJob<'static>, Seq0255<'static, U256<'static>>),
        BuildNoHashHasher<u64>,
    >,
    up: Arc<Mutex<Upstream>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
}

impl JobDeclarator {
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        config: ProxyConfig,
        up: Arc<Mutex<Upstream>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    ) -> Arc<Mutex<Self>> {
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Initiator(initiator)).await;

        let proxy_address = SocketAddr::new(
            IpAddr::from_str(&config.downstream_address).unwrap(),
            config.downstream_port,
        );

        info!(
            "JD proxy: setupconnection Proxy address: {:?}",
            proxy_address
        );

        SetupConnectionHandler::setup(&mut receiver, &mut sender, proxy_address)
            .await
            .unwrap();

        info!("JD CONNECTED");

        let min_extranonce_size = config.min_extranonce2_size;

        let self_ = Arc::new(Mutex::new(JobDeclarator {
            receiver,
            sender,
            allocated_tokens: vec![],
            req_ids: Id::new(),
            min_extranonce_size,
            last_declare_mining_job_sent: vec![],
            last_set_new_prev_hash: None,
            future_jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            up,
            task_collector,
        }));

        Self::allocate_tokens(&self_, 2).await;
        Self::on_upstream_message(self_.clone());
        self_
    }

    pub fn get_last_declare_job_sent(
        self_mutex: &Arc<Mutex<Self>>,
    ) -> (
        DeclareMiningJob<'static>,
        bool,
        u64,
        Seq0255<'static, U256<'static>>,
    ) {
        self_mutex
            .safe_lock(|s| match s.last_declare_mining_job_sent.len() {
                1 => s.last_declare_mining_job_sent.pop().unwrap(),
                _ => unreachable!(),
            })
            .unwrap()
    }

    #[async_recursion]
    pub async fn get_last_token(
        self_mutex: &Arc<Mutex<Self>>,
    ) -> AllocateMiningJobTokenSuccess<'static> {
        let token_len = self_mutex.safe_lock(|s| s.allocated_tokens.len()).unwrap();
        match token_len {
            0 => {
                {
                    let task = {
                        let self_mutex = self_mutex.clone();
                        tokio::task::spawn(async move {
                            Self::allocate_tokens(&self_mutex, 2).await;
                        })
                    };
                    self_mutex
                        .safe_lock(|s| {
                            s.task_collector
                                .safe_lock(|c| c.push(task.abort_handle()))
                                .unwrap()
                        })
                        .unwrap();
                }
                tokio::task::yield_now().await;
                Self::get_last_token(self_mutex).await
            }
            1 => {
                {
                    let task = {
                        let self_mutex = self_mutex.clone();
                        tokio::task::spawn(async move {
                            Self::allocate_tokens(&self_mutex, 1).await;
                        })
                    };
                    self_mutex
                        .safe_lock(|s| {
                            s.task_collector
                                .safe_lock(|c| c.push(task.abort_handle()))
                                .unwrap()
                        })
                        .unwrap();
                }
                self_mutex
                    .safe_lock(|s| s.allocated_tokens.pop())
                    .unwrap()
                    .unwrap()
            }
            _ => self_mutex
                .safe_lock(|s| s.allocated_tokens.pop())
                .unwrap()
                .unwrap(),
        }
    }

    pub async fn on_new_template(
        self_mutex: &Arc<Mutex<Self>>,
        template: NewTemplate<'static>,
        token: Vec<u8>,
        pool_output: Vec<u8>,
    ) {
        let (id, min_extranonce_size, sender) = self_mutex
            .safe_lock(|s| (s.req_ids.next(), s.min_extranonce_size, s.sender.clone()))
            .unwrap();
        let mut outputs = pool_output;
        let mut tp_outputs: Vec<u8> = template.coinbase_tx_outputs.to_vec();
        outputs.append(&mut tp_outputs);
        let declare_job = DeclareMiningJob {
            request_id: id,
            mining_job_token: token.try_into().unwrap(),
            version: template.version,
            coinbase_tx_version: template.coinbase_tx_version,
            coinbase_prefix: template.coinbase_prefix,
            coinbase_tx_input_n_sequence: template.coinbase_tx_input_sequence,
            coinbase_tx_value_remaining: template.coinbase_tx_value_remaining,
            coinbase_tx_outputs: outputs.try_into().unwrap(),
            coinbase_tx_locktime: template.coinbase_tx_locktime,
            min_extranonce_size,
            tx_short_hash_nonce: 0, // TODO should be sent to bitcoind when session start
            tx_short_hash_list: vec![].try_into().unwrap(), // TODO this come wither in a separeta message or in newtemplate
            tx_hash_list_hash: vec![0; 32].try_into().unwrap(), // TODO
            excess_data: vec![].try_into().unwrap(),
        };
        self_mutex
            .safe_lock(|s| {
                s.last_declare_mining_job_sent.push((
                    declare_job.clone(),
                    template.future_template,
                    template.template_id,
                    template.merkle_path,
                ))
            })
            .unwrap();
        let frame: StdFrame =
            PoolMessages::JobDeclaration(JobDeclaration::DeclareMiningJob(declare_job))
                .try_into()
                .unwrap();
        sender.send(frame.into()).await.unwrap();
    }

    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        let up = self_mutex.safe_lock(|s| s.up.clone()).unwrap();
        let main_task = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
                loop {
                    let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                    let message_type = incoming.get_header().unwrap().msg_type();
                    let payload = incoming.payload();

                    let next_message_to_send =
                        ParseServerJobDeclarationMessages::handle_message_job_declaration(
                            self_mutex.clone(),
                            message_type,
                            payload,
                        );
                    match next_message_to_send {
                        Ok(SendTo::None(Some(JobDeclaration::DeclareMiningJobSuccess(m)))) => {
                            let new_token = m.new_mining_job_token;
                            let (mut last_declare_mining_job_sent, is_future, id, merkle_path) =
                                Self::get_last_declare_job_sent(&self_mutex);
                            if is_future {
                                last_declare_mining_job_sent.mining_job_token = new_token;
                                self_mutex
                                    .safe_lock(|s| {
                                        s.future_jobs
                                            .insert(id, (last_declare_mining_job_sent, merkle_path))
                                    })
                                    .unwrap();
                            } else {
                                let set_new_prev_hash = self_mutex
                                    .safe_lock(|s| s.last_set_new_prev_hash.clone())
                                    .unwrap();
                                match set_new_prev_hash {
                                    Some(p) => Upstream::set_custom_jobs(&up, last_declare_mining_job_sent, p, merkle_path, new_token).await.unwrap(),
                                    None => panic!("Invalid state we received a NewTemplate not future, without having received a set new prev hash")
                                }
                            }
                        }
                        Ok(SendTo::None(None)) => (),
                        Ok(_) => unreachable!(),
                        Err(_) => todo!(),
                    }
                }
            })
        };
        self_mutex
            .safe_lock(|s| {
                s.task_collector
                    .safe_lock(|c| c.push(main_task.abort_handle()))
                    .unwrap()
            })
            .unwrap();
    }

    pub async fn on_set_new_prev_hash(
        self_mutex: &Arc<Mutex<Self>>,
        set_new_prev_hash: SetNewPrevHash<'static>,
    ) {
        let id = set_new_prev_hash.template_id;
        let future_job_tuple = self_mutex
            .safe_lock(|s| {
                s.last_set_new_prev_hash = Some(set_new_prev_hash.clone());
                match s.future_jobs.remove(&id) {
                    Some((job, merkle_path)) => {
                        s.future_jobs = HashMap::with_hasher(BuildNoHashHasher::default());
                        Some((job, s.up.clone(), merkle_path))
                    }
                    None => None,
                }
            })
            .unwrap();
        if let Some((job, up, merkle_path)) = future_job_tuple {
            // the declare_job token has already been signed in sefl.on_upstream_message
            // due to that we use job.token as signed_token
            let signed_token = job.mining_job_token.clone();
            Upstream::set_custom_jobs(&up, job, set_new_prev_hash, merkle_path, signed_token)
                .await
                .unwrap();
        };
    }

    async fn allocate_tokens(self_mutex: &Arc<Mutex<Self>>, token_to_allocate: u32) {
        for i in 0..token_to_allocate {
            let message = JobDeclaration::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: "todo".to_string().try_into().unwrap(),
                request_id: i,
            });
            let sender = self_mutex.safe_lock(|s| s.sender.clone()).unwrap();
            // Safe unwrap message is build above and is valid, below can never panic
            let frame: StdFrame = PoolMessages::JobDeclaration(message).try_into().unwrap();
            // TODO join re
            sender.send(frame.into()).await.unwrap();
        }
    }
}
