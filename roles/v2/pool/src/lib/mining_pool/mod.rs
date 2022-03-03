use async_std::net::TcpListener;
use async_std::prelude::*;
use async_std::task;
use codec_sv2::{HandshakeRole, Responder};
use network_helpers::Connection;

use crate::{EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use async_std::sync::Arc;
use codec_sv2::Frame;
use messages_sv2::common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream};
use messages_sv2::errors::Error;
use messages_sv2::handlers::mining::{ParseDownstreamMiningMessages, SendTo};
use messages_sv2::job_creator::JobsCreators;
use messages_sv2::mining_sv2::NewExtendedMiningJob;
use messages_sv2::mining_sv2::SetNewPrevHash as NewPrevHash;
use messages_sv2::parsers::{Mining, PoolMessages};
use messages_sv2::routing_logic::MiningRoutingLogic;
use messages_sv2::template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use messages_sv2::utils::Id;
use messages_sv2::utils::Mutex;
use std::collections::HashMap;
use std::convert::TryInto;

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
    channel_ids: Id,
}

/// Accept downstream connection
pub struct Pool {
    /// Downstreams that are not HOM
    group_downstreams: HashMap<u32, Arc<Mutex<Downstream>>>,
    /// Downstreams that are HOM
    hom_downstreams: HashMap<u32, Arc<Mutex<Downstream>>>,
    hom_ids: Arc<Mutex<Id>>,
    group_ids: Arc<Mutex<Id>>,
    job_creators: Arc<Mutex<JobsCreators>>,
}

impl Downstream {
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        group_ids: Arc<Mutex<Id>>,
        _hom_ids: Arc<Mutex<Id>>,
        job_creators: Arc<Mutex<JobsCreators>>,
    ) -> Arc<Mutex<Self>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let downstream_data =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender)
                .await
                .unwrap();
        let id = match downstream_data.header_only {
            false => group_ids.safe_lock(|id| id.next()).unwrap(),
            true => {
                //_hom_ids.safe_lock(|id| id.next()).unwrap();
                panic!("Downstream standard channel not supported");
            }
        };
        job_creators
            .safe_lock(|j| j.new_group_channel(id, downstream_data.version_rolling))
            .unwrap();
        let self_ = Arc::new(Mutex::new(Downstream {
            id,
            receiver,
            sender,
            downstream_data,
            channel_ids: Id::new(),
        }));
        let cloned = self_.clone();
        task::spawn(async move {
            loop {
                let receiver = cloned.safe_lock(|d| d.receiver.clone()).unwrap();
                let incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                Downstream::next(cloned.clone(), incoming).await
            }
        });
        self_
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) {
        let message_type = incoming.get_header().unwrap().msg_type();
        let payload = incoming.payload();
        let next_message_to_send = ParseDownstreamMiningMessages::handle_message_mining(
            self_mutex.clone(),
            message_type,
            payload,
            MiningRoutingLogic::None,
        );
        match next_message_to_send {
            Ok(SendTo::RelayNewMessage(_, message)) => {
                let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into().unwrap();
                let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
                sender.send(sv2_frame.into()).await.unwrap();
            }
            Ok(_) => panic!(),
            Err(Error::UnexpectedMessage) => todo!(),
            Err(_) => todo!(),
        }

        //TODO
    }

    pub async fn on_new_prev_hash(
        self_: Arc<Mutex<Self>>,
        message: NewPrevHash<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::Mining(Mining::SetNewPrevHash(message))
            .try_into()
            .unwrap();
        let sender = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }

    pub async fn on_new_extended_job(
        self_: Arc<Mutex<Self>>,
        message: NewExtendedMiningJob<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = PoolMessages::Mining(Mining::NewExtendedMiningJob(message))
            .try_into()
            .unwrap();
        let sender = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
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
    async fn accept_incoming_connection(self_: Arc<Mutex<Pool>>) {
        let listner = TcpListener::bind(crate::ADDR).await.unwrap();
        let mut incoming = listner.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let responder = Responder::from_authority_kp(
                &crate::AUTHORITY_PUBLIC_K[..],
                &crate::AUTHORITY_PRIVATE_K[..],
                crate::CERT_VALIDITY,
            );
            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await;
            let group_ids = self_.safe_lock(|s| s.group_ids.clone()).unwrap();
            let hom_ids = self_.safe_lock(|s| s.hom_ids.clone()).unwrap();
            let job_creators = self_.safe_lock(|s| s.job_creators.clone()).unwrap();
            let downstream =
                Downstream::new(receiver, sender, group_ids, hom_ids, job_creators).await;
            let (is_header_only, id) = downstream
                .safe_lock(|d| (d.downstream_data.header_only, d.id))
                .unwrap();
            self_
                .safe_lock(|p| {
                    if is_header_only {
                        p.hom_downstreams.insert(id, downstream);
                    } else {
                        p.group_downstreams.insert(id, downstream);
                    }
                })
                .unwrap();
        }
    }

    async fn on_new_prev_hash(self_: Arc<Mutex<Self>>, rx: Receiver<SetNewPrevHash<'static>>) {
        while let Ok(new_prev_hash) = rx.recv().await {
            let hom_downstreams: Vec<Arc<Mutex<Downstream>>> = self_
                .safe_lock(|s| s.hom_downstreams.iter().map(|d| d.1.clone()).collect())
                .unwrap();
            let group_downstreams: Vec<Arc<Mutex<Downstream>>> = self_
                .safe_lock(|s| s.group_downstreams.iter().map(|d| d.1.clone()).collect())
                .unwrap();
            for downstream in [&hom_downstreams[..], &group_downstreams[..]].concat() {
                let message = NewPrevHash {
                    channel_id: 0,
                    job_id: 0,
                    prev_hash: new_prev_hash.prev_hash.clone(),
                    min_ntime: 0,
                    nbits: new_prev_hash.n_bits,
                };
                Downstream::on_new_prev_hash(downstream.clone(), message)
                    .await
                    .unwrap();
            }
        }
    }

    async fn on_new_template(self_: Arc<Mutex<Self>>, rx: Receiver<NewTemplate<'_>>) {
        while let Ok(mut new_template) = rx.recv().await {
            let job_creators = self_.safe_lock(|s| s.job_creators.clone()).unwrap();
            let mut new_jobs = job_creators
                .safe_lock(|j| j.on_new_template(&mut new_template))
                .unwrap();
            let group_downstreams: Vec<Arc<Mutex<Downstream>>> = self_
                .safe_lock(|s| s.group_downstreams.iter().map(|d| d.1.clone()).collect())
                .unwrap();
            // TODO add standard channel downstream
            for downstream in group_downstreams {
                let channel_id = downstream.safe_lock(|x| x.id).unwrap();
                let extended_job = new_jobs.remove(&channel_id).unwrap();
                Downstream::on_new_extended_job(downstream, extended_job)
                    .await
                    .unwrap();
            }
        }
    }

    pub async fn start(
        new_template_rx: Receiver<NewTemplate<'static>>,
        new_prev_hash_rx: Receiver<SetNewPrevHash<'static>>,
    ) {
        //let group_id_generator = Arc::new(Mutex::new(Id::new()));
        let pool = Arc::new(Mutex::new(Pool {
            group_downstreams: HashMap::new(),
            hom_downstreams: HashMap::new(),
            hom_ids: Arc::new(Mutex::new(Id::new())),
            group_ids: Arc::new(Mutex::new(Id::new())),
            job_creators: Arc::new(Mutex::new(JobsCreators::new(
                crate::BLOCK_REWARD,
                crate::new_pub_key(),
            ))),
        }));

        let cloned = pool.clone();
        let cloned2 = pool.clone();
        let cloned3 = pool.clone();

        task::spawn(async move {
            Self::accept_incoming_connection(cloned).await;
        });

        task::spawn(async {
            Self::on_new_prev_hash(cloned2, new_prev_hash_rx).await;
        });

        task::spawn(async move {
            Self::on_new_template(cloned3, new_template_rx).await;
        })
        .await;
    }
}
