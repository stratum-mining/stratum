use codec_sv2::{HandshakeRole, Responder};
use network_helpers::noise_connection_tokio::Connection;
use tokio::{net::TcpListener, task};

use crate::{Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use codec_sv2::Frame;
use roles_logic_sv2::{
    channel_logic::channel_factory::PoolChannelFactory,
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::mining::{ParseDownstreamMiningMessages, SendTo},
    job_creator::{JobsCreators, ScriptKind},
    mining_sv2::{ExtendedExtranonce, SetNewPrevHash as SetNPH},
    parsers::{Mining, PoolMessages},
    routing_logic::MiningRoutingLogic,
    template_distribution_sv2::{NewTemplate, SetNewPrevHash, SubmitSolution},
    utils::Mutex,
};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tracing::{debug, error, info};

pub mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub mod message_handler;

#[derive(Debug)]
pub struct Downstream {
    // Either group or channel id
    id: u32,
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    downstream_data: CommonDownstreamData,
    solution_sender: Sender<SubmitSolution<'static>>,
    channel_factory: Arc<Mutex<PoolChannelFactory>>,
}

/// Accept downstream connection
pub struct Pool {
    downstreams: HashMap<u32, Arc<Mutex<Downstream>>>,
    solution_sender: Sender<SubmitSolution<'static>>,
    new_template_processed: bool,
    channel_factory: Arc<Mutex<PoolChannelFactory>>,
    last_prev_hash_template_id: u64,
}

impl Downstream {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        solution_sender: Sender<SubmitSolution<'static>>,
        pool: Arc<Mutex<Pool>>,
        channel_factory: Arc<Mutex<PoolChannelFactory>>,
    ) -> Arc<Mutex<Self>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let downstream_data =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender)
                .await
                .unwrap();
        let id = match downstream_data.header_only {
            false => channel_factory.safe_lock(|c| c.new_group_id()).unwrap(),
            true => channel_factory
                .safe_lock(|c| c.new_standard_id_for_hom())
                .unwrap(),
        };

        let self_ = Arc::new(Mutex::new(Downstream {
            id,
            receiver,
            sender,
            downstream_data,
            solution_sender,
            channel_factory,
        }));

        let cloned = self_.clone();

        task::spawn(async move {
            debug!("Starting up downstream receiver");
            loop {
                let receiver = cloned.safe_lock(|d| d.receiver.clone()).unwrap();
                match receiver.recv().await {
                    Ok(received) => {
                        let received: Result<StdFrame, _> = received.try_into();
                        match received {
                            Ok(std_frame) => Downstream::next(cloned.clone(), std_frame).await,
                            _ => todo!(),
                        }
                    }
                    _ => {
                        pool.safe_lock(|p| p.downstreams.remove(&id)).unwrap();
                        error!("Downstream {} disconnected", id);
                        break;
                    }
                }
            }
        });
        self_
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        debug!(
            "Received downstream message type: {:?}, payload: {:?}",
            message_type, payload
        );
        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            MiningRoutingLogic::None,
        );
        Self::match_send_to(self_mutex, next_message_to_send).await;
    }

    #[async_recursion::async_recursion]
    async fn match_send_to(self_: Arc<Mutex<Self>>, send_to: Result<SendTo<()>, Error>) {
        match send_to {
            Ok(SendTo::Respond(message)) => {
                debug!("Sending to downstream: {:?}", message);
                Self::send(self_, message)
                    .await
                    .expect("Failed to send downstream message");
            }
            Ok(SendTo::Multiple(messages)) => {
                debug!("Sending multiple messages to downstream");
                for message in messages {
                    debug!("Sending downstream message: {:?}", message);
                    Self::match_send_to(self_.clone(), Ok(message)).await;
                }
            }
            Ok(SendTo::None(_)) => (),
            Ok(_) => panic!(),
            Err(Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }
    }

    async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::Mining<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }
}
impl IsDownstream for Downstream {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        self.downstream_data
    }
}

impl IsMiningDownstream for Downstream {}

impl Pool {
    #[cfg(feature = "test_only_allow_unencrypted")]
    async fn accept_incoming_plain_connection(self_: Arc<Mutex<Pool>>, config: Configuration) {
        let listner = TcpListener::bind(&config.test_only_listen_adress_plain)
            .await
            .unwrap();
        info!(
            "Listening for unencrypted connection on: {}",
            config.test_only_listen_adress_plain
        );
        while let Ok((stream, _)) = listner.accept().await {
            debug!("New connection from {}", stream.peer_addr().unwrap());

            // Uncomment to allow unencrypted connections
            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                network_helpers::plain_connection_tokio::PlainConnection::new(stream).await;
            Self::accept_incoming_connection_(self_.clone(), receiver, sender).await;
        }
    }

    async fn accept_incoming_connection(self_: Arc<Mutex<Pool>>, config: Configuration) {
        let listner = TcpListener::bind(&config.listen_address).await.unwrap();
        info!(
            "Listening for encrypted connection on: {}",
            config.listen_address
        );
        while let Ok((stream, _)) = listner.accept().await {
            debug!("New connection from {}", stream.peer_addr().unwrap());

            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            )
            .unwrap();

            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;
            Self::accept_incoming_connection_(self_.clone(), receiver, sender).await;
        }
    }

    async fn accept_incoming_connection_(
        self_: Arc<Mutex<Pool>>,
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
    ) {
        let solution_sender = self_.safe_lock(|p| p.solution_sender.clone()).unwrap();

        let channel_factory = self_.safe_lock(|s| s.channel_factory.clone()).unwrap();

        let downstream = Downstream::new(
            receiver,
            sender,
            solution_sender,
            self_.clone(),
            channel_factory,
        )
        .await;

        let (_, channel_id) = downstream
            .safe_lock(|d| (d.downstream_data.header_only, d.id))
            .unwrap();

        self_
            .safe_lock(|p| {
                p.downstreams.insert(channel_id, downstream);
            })
            .unwrap();
    }

    async fn on_new_prev_hash(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<SetNewPrevHash<'static>>,
        sender_message_received_signal: Sender<()>,
    ) {
        while let Ok(new_prev_hash) = rx.recv().await {
            debug!("New prev hash received: {:?}", new_prev_hash);
            self_
                .safe_lock(|s| {
                    s.last_prev_hash_template_id = new_prev_hash.template_id;
                })
                .unwrap();
            let x = self_
                .safe_lock(|s| {
                    s.channel_factory
                        .safe_lock(|f| f.on_new_prev_hash_from_tp(&new_prev_hash))
                        .unwrap()
                })
                .unwrap();
            match x {
                Ok(job_id) => {
                    let downstreams = self_.safe_lock(|s| s.downstreams.clone()).unwrap();
                    for (channel_id, downtream) in downstreams {
                        let message = Mining::SetNewPrevHash(SetNPH {
                            channel_id,
                            job_id,
                            prev_hash: new_prev_hash.prev_hash.clone(),
                            min_ntime: new_prev_hash.header_timestamp,
                            nbits: new_prev_hash.n_bits,
                        });
                        Downstream::match_send_to(downtream.clone(), Ok(SendTo::Respond(message)))
                            .await;
                    }
                    sender_message_received_signal.send(()).await.unwrap();
                }
                Err(_) => todo!(),
            }
        }
    }

    async fn on_new_template(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<NewTemplate<'static>>,
        sender_message_received_signal: Sender<()>,
    ) {
        let mut first_done = false;
        while let Ok(mut new_template) = rx.recv().await {
            debug!(
                "New template received, creating a new mining job(s): {:?}",
                new_template
            );
            // TODO temporary fix to bitcoind error that send a non future new template before the
            // sending the p hash
            if !first_done {
                new_template.future_template = true;
                first_done = true;
            }

            let channel_factory = self_.safe_lock(|s| s.channel_factory.clone()).unwrap();
            let mut messages = channel_factory
                .safe_lock(|cf| cf.on_new_template(&mut new_template).unwrap())
                .unwrap();
            let downstreams = self_.safe_lock(|s| s.downstreams.clone()).unwrap();
            for (channel_id, downtream) in downstreams {
                let to_send = messages.remove(&channel_id).unwrap();
                Downstream::match_send_to(downtream.clone(), Ok(SendTo::Respond(to_send))).await;
            }
            self_
                .safe_lock(|s| s.new_template_processed = true)
                .unwrap();
            sender_message_received_signal.send(()).await.unwrap();
        }
    }

    pub async fn start(
        config: Configuration,
        new_template_rx: Receiver<NewTemplate<'static>>,
        new_prev_hash_rx: Receiver<SetNewPrevHash<'static>>,
        solution_sender: Sender<SubmitSolution<'static>>,
        sender_message_received_signal: Sender<()>,
    ) {
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1 = std::ops::Range { start: 0, end: 16 };
        let range_2 = std::ops::Range { start: 16, end: 32 };
        let script_kind = ScriptKind::PayToPubKey(crate::new_pub_key().wpubkey_hash().unwrap());
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
        let extranonces =
            ExtendedExtranonce::new(range_0.clone(), range_1.clone(), range_2.clone());
        let creator = JobsCreators::new(crate::BLOCK_REWARD, script_kind.clone(), 32);
        let share_per_min = 1.0;
        let kind = roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::Pool;
        let channel_factory = Arc::new(Mutex::new(PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            kind,
        )));
        let pool = Arc::new(Mutex::new(Pool {
            downstreams: HashMap::new(),
            solution_sender,
            new_template_processed: false,
            channel_factory,
            last_prev_hash_template_id: 0,
        }));

        let cloned = pool.clone();
        let cloned2 = pool.clone();
        let cloned3 = pool.clone();
        #[cfg(feature = "test_only_allow_unencrypted")]
        let cloned4 = pool.clone();

        info!("Starting up pool listener");
        task::spawn(Self::accept_incoming_connection(cloned, config.clone()));
        #[cfg(feature = "test_only_allow_unencrypted")]
        task::spawn(Self::accept_incoming_plain_connection(cloned4, config));

        let cloned = sender_message_received_signal.clone();
        task::spawn(async {
            Self::on_new_prev_hash(cloned2, new_prev_hash_rx, cloned).await;
        });

        let _ = task::spawn(async move {
            Self::on_new_template(cloned3, new_template_rx, sender_message_received_signal).await;
        })
        .await;
    }
}
