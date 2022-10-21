pub mod message_handler;
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use network_helpers::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::SendTo_,
    job_negotiation_sv2::{AllocateMiningJobToken, AllocateMiningJobTokenSuccess, CommitMiningJob},
    parsers::{JobNegotiation, PoolMessages},
    utils::Mutex,
};
use std::{collections::HashMap, convert::TryInto, str::FromStr};
use tracing::{debug, info};


use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::job_negotiation::ParseServerJobNegotiationMessages,
    template_distribution_sv2::{CoinbaseOutputDataSize, NewTemplate, SetNewPrevHash},

};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpStream, task};


pub type Message = PoolMessages<'static>;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
pub type EitherFrame = StandardEitherFrame<PoolMessages<'static>>;
pub type StdFrame = StandardSv2Frame<Message>;
use crate::Config;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub struct JobNegotiator {
    sender: Sender<StandardEitherFrame<PoolMessages<'static>>>,
    receiver: Receiver<StandardEitherFrame<PoolMessages<'static>>>,
    receiver_new_template: Receiver<NewTemplate<'static>>,
    receiver_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    last_new_template: Option<NewTemplate<'static>>,
    set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    future_templates: Vec<NewTemplate<'static>>,
    sender_coinbase_output_max_additional_size: Sender<CoinbaseOutputDataSize>,
    allocate_mining_job_message: AllocateMiningJobTokenSuccess<'static>,
}

impl JobNegotiator {
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        receiver_new_template: Receiver<NewTemplate<'static>>,
        receiver_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        sender_coinbase_output_max_additional_size: Sender<CoinbaseOutputDataSize>,

    ) {
        let stream = TcpStream::connect(address).await.unwrap();
        let initiator = Initiator::from_raw_k(authority_public_key).unwrap();
        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            Connection::new(stream, HandshakeRole::Initiator(initiator)).await;

        let config_file: String = std::fs::read_to_string("proxy-config.toml").unwrap();
        let config: Config = toml::from_str(&config_file).unwrap();
        let proxy_address = SocketAddr::new(
            IpAddr::from_str(&config.listen_address).unwrap(),
            config.listen_mining_port,
        );

        info!(

            "JN proxy: setupconnection \nProxy address: {:?}",
            proxy_address
        );

        SetupConnectionHandler::setup(&mut receiver, &mut sender, proxy_address)
            .await
            .unwrap();

        info!("\nJN CONNECTED");


        let self_ = Arc::new(Mutex::new(JobNegotiator {
            sender,
            receiver,
            receiver_new_template,
            receiver_set_new_prev_hash,
            last_new_template: None,
            set_new_prev_hash: None,
            future_templates: Vec::new(),
            sender_coinbase_output_max_additional_size,
            allocate_mining_job_message: AllocateMiningJobTokenSuccess {
                request_id: 0,
                mining_job_token: vec![].try_into().unwrap(),
                coinbase_output_max_additional_size: 0,
                async_mining_allowed: false,
            },

        }));

        let allocate_token_message =
            JobNegotiation::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: "4ss0".to_string().try_into().unwrap(),
                request_id: 1,
            });


        Self::send(self_.clone(), allocate_token_message)
            .await
            .unwrap();

        let cloned = self_.clone();
        Self::on_upstream_message(cloned.clone());
        Self::on_new_template(cloned.clone());
        Self::on_new_prev_hash(cloned.clone());

    }

    pub fn on_new_template(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver_new_tp = self_mutex
                    .clone()
                    .safe_lock(|d| d.receiver_new_template.clone())
                    .unwrap();
                let incoming_new_template: NewTemplate =
                    receiver_new_tp.recv().await.unwrap().try_into().unwrap();
                println!(
                    "\n\nNew template recieved in JN {:?}\n\n",
                    incoming_new_template
                );
                if incoming_new_template.clone().future_template {
                    self_mutex
                        .clone()
                        .safe_lock(|t| {
                            t.future_templates.push(incoming_new_template.clone());
                        })
                        .unwrap();
                    if JobNegotiator::is_for_future_template(self_mutex.clone()) {
                        JobNegotiator::make_job(self_mutex.clone()).await;
                    }
                } else {
                    self_mutex
                        .clone()
                        .safe_lock(|t| {
                            t.last_new_template = Some(incoming_new_template);
                        })
                        .unwrap();
                    JobNegotiator::make_job(self_mutex.clone()).await;
                }
            }
        });
    }

    pub fn on_new_prev_hash(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver_new_ph = self_mutex
                    .clone()
                    .safe_lock(|d| d.receiver_set_new_prev_hash.clone())
                    .unwrap();
                let incoming_set_new_ph: SetNewPrevHash =
                    receiver_new_ph.recv().await.unwrap().try_into().unwrap();
                println!(
                    "\n\nSet new prev hash recieved in JN {:?}\n\n",
                    incoming_set_new_ph
                );
                self_mutex
                    .clone()
                    .safe_lock(|t| {
                        t.set_new_prev_hash = Some(incoming_set_new_ph);
                    })
                    .unwrap();
                if JobNegotiator::is_for_future_template(self_mutex.clone()) {
                    JobNegotiator::make_job(self_mutex.clone()).await;
                }

            }
        });
    }

    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
                let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                println!("next message {:?}", incoming);
                JobNegotiator::next(self_mutex.clone(), incoming).await
            }
        });
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send =
            ParseServerJobNegotiationMessages::handle_message_job_negotiation(
                self_mutex.clone(),
                message_type,
                payload,
            );
        match next_message_to_send {
            Ok(SendTo::Respond(message)) => Self::send(self_mutex, message).await.unwrap(),
            Ok(SendTo::None(Some(m))) => match m {
                // Coinbase_output_max_additional_size is received from pool ...
                JobNegotiation::AllocateMiningJobTokenSuccess(m) => {
                    let sender = self_mutex
                        .safe_lock(|s| s.sender_coinbase_output_max_additional_size.clone())
                        .unwrap();
                    let coinbase_output_max_additional_size = CoinbaseOutputDataSize {
                        coinbase_output_max_additional_size: m.coinbase_output_max_additional_size,
                    };
                    // ... than coinbase_output_max_additional_size is sent to TP_receiver
                    sender
                        .send(coinbase_output_max_additional_size)
                        .await
                        .unwrap();
                    self_mutex
                        .safe_lock(|j| j.allocate_mining_job_message = m)
                        .unwrap();
                }
                _ => unreachable!(),
            },
            Ok(_) => print!("MVP2 ENDS HERE"),

            Err(_) => todo!(),
        }
    }

    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: JobNegotiation<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::JobNegotiation(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }

    pub async fn make_job(self_mutex: Arc<Mutex<Self>>) {
        let set_new_prev_hash = self_mutex
            .safe_lock(|j| j.set_new_prev_hash.clone().unwrap())
            .unwrap();
        let last_new_template = self_mutex
            .safe_lock(|j| j.last_new_template.clone().unwrap())
            .unwrap();
        info!(
            "\n\nJOB TO MAKE --- SET NEW PREV HASH:\n{:?}\nNEW TEMPLATE:\n{:?}\n\n",
            set_new_prev_hash, last_new_template
        );
        // send commit mining job to pool
        //JobNegotiator::send_commit_mining_job(), use jobNegotiator.message_request_id to map the right message
        let message = CommitMiningJob {
            request_id: self_mutex
                .safe_lock(|j| j.allocate_mining_job_message.request_id)
                .unwrap(),
            mining_job_token: self_mutex
                .safe_lock(|j| j.allocate_mining_job_message.mining_job_token.clone())
                .unwrap(),
            version: 2,
            coinbase_tx_version: last_new_template.coinbase_tx_version,
            coinbase_prefix: last_new_template.coinbase_prefix,
            coinbase_tx_input_n_sequence: last_new_template.coinbase_tx_input_sequence,
            coinbase_tx_value_remaining: last_new_template.coinbase_tx_value_remaining,
            coinbase_tx_outputs: last_new_template.coinbase_tx_outputs,
            coinbase_tx_locktime: last_new_template.coinbase_tx_locktime,
            min_extranonce_size: 0,
            tx_short_hash_nonce: 0,
            /// Only for MVP2: must be filled with right values for production,
            /// this values are needed for block propagation
            tx_short_hash_list: vec![].try_into().unwrap(),
            tx_hash_list_hash: [0; 32].try_into().unwrap(),
            excess_data: vec![].try_into().unwrap(),
        };
        Self::send(
            self_mutex.clone(),
            roles_logic_sv2::parsers::JobNegotiation::CommitMiningJob(message),
        )
        .await
        .unwrap();
        // create a Job
        // send the Job to mining devices

        // RESET future template
        self_mutex
            .safe_lock(|j| j.future_templates = Vec::new())
            .unwrap();
        info!("\n\nfuture templates cleared\n\n");
    }

    pub fn is_for_future_template(self_mutex: Arc<Mutex<Self>>) -> bool {
        // given set new prev hash, finds the future template and put it in the last template in JN
        let vec_future_templates = self_mutex
            .clone()
            .safe_lock(|j| j.future_templates.clone())
            .unwrap();
        let prev_hash_template_id = self_mutex
            .safe_lock(|j| j.set_new_prev_hash.clone().unwrap().template_id.clone())
            .unwrap();
        print!(
            "\n\nNEW VEC OF FUTURE TEMPLATES {:?}\n\n",
            vec_future_templates
        );
        for future_template in vec_future_templates.iter() {
            info!(
                "\n\nPREV HASH TEMPLATE ID: {:?}\nFUTURE TEMPLATE ID: {:?}\n\n",
                prev_hash_template_id, future_template.template_id
            );
            if prev_hash_template_id == future_template.template_id {
                self_mutex
                    .safe_lock(|j| j.last_new_template = Some(future_template.clone()))
                    .unwrap();
                info!(
                    "Last New Template is : {:?}",
                    self_mutex.safe_lock(|j| j.last_new_template.clone().unwrap())
                );
                println!(
                    "\nGOT a future template: {:?}\nPrev hash ID: {:?}\n",
                    future_template.clone(),
                    prev_hash_template_id
                );
                return true;
            }
        }
        false
    }
}
