use codec_sv2::{HandshakeRole, Responder};
use network_helpers::noise_connection_tokio::Connection;
use tokio::{net::TcpListener, task};

use crate::{
    error::{PoolError, PoolResult},
    status, Configuration, EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use bitcoin::{Script, TxOut, hashes::hex::ToHex};
use codec_sv2::Frame;
use error_handling::handle_result;
use roles_logic_sv2::{
    channel_logic::channel_factory::PoolChannelFactory,
    common_properties::{CommonDownstreamData, IsDownstream, IsMiningDownstream},
    errors::Error,
    handlers::mining::{ParseDownstreamMiningMessages, SendTo},
    job_creator::JobsCreators,
    mining_sv2::{ExtendedExtranonce, SetNewPrevHash as SetNPH},
    parsers::{Mining, PoolMessages},
    routing_logic::MiningRoutingLogic,
    template_distribution_sv2::{NewTemplate, SetNewPrevHash, SubmitSolution},
    utils::Mutex,
};
use std::{collections::HashMap, convert::TryInto, sync::Arc, str::FromStr};
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
    status_tx: status::Sender,
}

impl Downstream {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mut receiver: Receiver<EitherFrame>,
        mut sender: Sender<EitherFrame>,
        solution_sender: Sender<SubmitSolution<'static>>,
        pool: Arc<Mutex<Pool>>,
        channel_factory: Arc<Mutex<PoolChannelFactory>>,
        status_tx: status::Sender,
    ) -> PoolResult<Arc<Mutex<Self>>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let downstream_data =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender).await?;

        let id = match downstream_data.header_only {
            false => channel_factory.safe_lock(|c| c.new_group_id())?,
            true => channel_factory.safe_lock(|c| c.new_standard_id_for_hom())?,
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
            let receiver_res = cloned
                .safe_lock(|d| d.receiver.clone())
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let receiver = match receiver_res {
                Ok(recv) => recv,
                Err(e) => {
                    if let Err(e) = status_tx
                        .send(status::Status {
                            state: status::State::Healthy(format!(
                                "Downstream connection dropped: {}",
                                e
                            )),
                        })
                        .await
                    {
                        error!("Encountered Error but status channel is down: {}", e);
                    }

                    return;
                }
            };
            loop {
                match receiver.recv().await {
                    Ok(received) => {
                        let received: Result<StdFrame, _> = received
                            .try_into()
                            .map_err(|e| PoolError::Codec(codec_sv2::Error::FramingSv2Error(e)));
                        let std_frame = handle_result!(status_tx, received);
                        handle_result!(
                            status_tx,
                            Downstream::next(cloned.clone(), std_frame).await
                        );
                    }
                    _ => {
                        let res = pool
                            .safe_lock(|p| p.downstreams.remove(&id))
                            .map_err(|e| PoolError::PoisonLock(e.to_string()));
                        handle_result!(status_tx, res);
                        error!("Downstream {} disconnected", id);
                        break;
                    }
                }
            }
        });
        Ok(self_)
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) -> PoolResult<()> {
        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Framing(String::from("No header set")))?
            .msg_type();
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
        Self::match_send_to(self_mutex, next_message_to_send).await?;
        Ok(())
    }

    #[async_recursion::async_recursion]
    async fn match_send_to(
        self_: Arc<Mutex<Self>>,
        send_to: Result<SendTo<()>, Error>,
    ) -> PoolResult<()> {
        match send_to {
            Ok(SendTo::Respond(message)) => {
                debug!("Sending to downstream: {:?}", message);
                Self::send(self_, message).await?;
            }
            Ok(SendTo::Multiple(messages)) => {
                debug!("Sending multiple messages to downstream");
                for message in messages {
                    debug!("Sending downstream message: {:?}", message);
                    Self::match_send_to(self_.clone(), Ok(message)).await?;
                }
            }
            Ok(SendTo::None(_)) => {}
            Ok(_) => panic!(),
            Err(Error::UnexpectedMessage(_message_type)) => todo!(),
            Err(_) => todo!(),
        }
        Ok(())
    }

    async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::Mining<'static>,
    ) -> PoolResult<()> {
        let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into()?;
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone())?;
        sender.send(sv2_frame.into()).await?;
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

    async fn accept_incoming_connection(
        self_: Arc<Mutex<Pool>>,
        config: Configuration,
    ) -> PoolResult<()> {
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;
        let listener = TcpListener::bind(&config.listen_address).await?;
        info!(
            "Listening for encrypted connection on: {}",
            config.listen_address
        );
        while let Ok((stream, _)) = listener.accept().await {
            debug!(
                "New connection from {:?}",
                stream.peer_addr().map_err(PoolError::Io)
            );

            let responder = Responder::from_authority_kp(
                config.authority_public_key.clone().into_inner().as_bytes(),
                config.authority_secret_key.clone().into_inner().as_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            );
            match responder {
                Ok(resp) => {
                    let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                        Connection::new(stream, HandshakeRole::Responder(resp)).await;
                    handle_result!(
                        status_tx,
                        Self::accept_incoming_connection_(self_.clone(), receiver, sender).await
                    );
                }
                Err(_e) => {
                    todo!()
                }
            }
        }
        Ok(())
    }

    async fn accept_incoming_connection_(
        self_: Arc<Mutex<Pool>>,
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
    ) -> PoolResult<()> {
        let solution_sender = self_.safe_lock(|p| p.solution_sender.clone())?;
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;
        let channel_factory = self_.safe_lock(|s| s.channel_factory.clone())?;

        let downstream = Downstream::new(
            receiver,
            sender,
            solution_sender,
            self_.clone(),
            channel_factory,
            // convert Listener variant to Downstream variant
            status_tx.listener_to_connection(),
        )
        .await?;

        let (_, channel_id) = downstream.safe_lock(|d| (d.downstream_data.header_only, d.id))?;

        self_.safe_lock(|p| {
            p.downstreams.insert(channel_id, downstream);
        })?;
        Ok(())
    }

    async fn on_new_prev_hash(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<SetNewPrevHash<'static>>,
        sender_message_received_signal: Sender<()>,
    ) -> PoolResult<()> {
        let status_tx = self_
            .safe_lock(|s| s.status_tx.clone())
            .map_err(|e| PoolError::PoisonLock(e.to_string()))?;
        while let Ok(new_prev_hash) = rx.recv().await {
            debug!("New prev hash received: {:?}", new_prev_hash);
            let res = self_
                .safe_lock(|s| {
                    s.last_prev_hash_template_id = new_prev_hash.template_id;
                })
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            handle_result!(status_tx, res);

            let job_id_res = self_
                .safe_lock(|s| {
                    s.channel_factory
                        .safe_lock(|f| f.on_new_prev_hash_from_tp(&new_prev_hash))
                        .map_err(|e| PoolError::PoisonLock(e.to_string()))
                })
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let job_id = handle_result!(status_tx, handle_result!(status_tx, job_id_res));

            match job_id {
                Ok(job_id) => {
                    let downstreams = self_
                        .safe_lock(|s| s.downstreams.clone())
                        .map_err(|e| PoolError::PoisonLock(e.to_string()));
                    let downstreams = handle_result!(status_tx, downstreams);

                    for (channel_id, downtream) in downstreams {
                        let message = Mining::SetNewPrevHash(SetNPH {
                            channel_id,
                            job_id,
                            prev_hash: new_prev_hash.prev_hash.clone(),
                            min_ntime: new_prev_hash.header_timestamp,
                            nbits: new_prev_hash.n_bits,
                        });
                        let res = Downstream::match_send_to(
                            downtream.clone(),
                            Ok(SendTo::Respond(message)),
                        )
                        .await;
                        handle_result!(status_tx, res);
                    }
                    handle_result!(status_tx, sender_message_received_signal.send(()).await);
                }
                Err(_) => todo!(),
            }
        }
        Ok(())
    }

    async fn on_new_template(
        self_: Arc<Mutex<Self>>,
        rx: Receiver<NewTemplate<'static>>,
        sender_message_received_signal: Sender<()>,
    ) -> PoolResult<()> {
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;
        let channel_factory = self_.safe_lock(|s| s.channel_factory.clone())?;
        while let Ok(mut new_template) = rx.recv().await {
            debug!(
                "New template received, creating a new mining job(s): {:?}",
                new_template
            );

            let messages = channel_factory
                .safe_lock(|cf| cf.on_new_template(&mut new_template))
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let messages = handle_result!(status_tx, messages);
            let mut messages = handle_result!(status_tx, messages);

            let downstreams = self_
                .safe_lock(|s| s.downstreams.clone())
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            let downstreams = handle_result!(status_tx, downstreams);

            for (channel_id, downtream) in downstreams {
                if let Some(to_send) = messages.remove(&channel_id) {
                    if let Err(e) =
                        Downstream::match_send_to(downtream.clone(), Ok(SendTo::Respond(to_send)))
                            .await
                    {
                        error!("Unknown template provider message: {:?}", e);
                    }
                }
            }
            let res = self_
                .safe_lock(|s| s.new_template_processed = true)
                .map_err(|e| PoolError::PoisonLock(e.to_string()));
            handle_result!(status_tx, res);

            handle_result!(status_tx, sender_message_received_signal.send(()).await);
        }
        Ok(())
    }

    pub fn start(
        config: Configuration,
        new_template_rx: Receiver<NewTemplate<'static>>,
        new_prev_hash_rx: Receiver<SetNewPrevHash<'static>>,
        solution_sender: Sender<SubmitSolution<'static>>,
        sender_message_received_signal: Sender<()>,
        status_tx: status::Sender,
    ) {
        let extranonce_len = 32;
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1 = std::ops::Range { start: 0, end: 16 };
        let range_2 = std::ops::Range {
            start: 16,
            end: extranonce_len,
        };
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
        let pool_coinbase_outputs = config.coinbase_outputs.iter().map(|pub_key_wrapper| {
            TxOut {
                // value will be updated by the addition of `ChannelFactory::split_outputs()` in PR #422
                value: crate::BLOCK_REWARD,
                script_pubkey: Script::new_p2pk(&pub_key_wrapper.pub_key),
            }
        }).collect();
        println!("PUB KEY: {:?}", pool_coinbase_outputs);
        let extranonces = ExtendedExtranonce::new(range_0, range_1, range_2);
        let creator = JobsCreators::new(extranonce_len as u8);
        let share_per_min = 1.0;
        let kind = roles_logic_sv2::channel_logic::channel_factory::ExtendedChannelKind::Pool;
        let channel_factory = Arc::new(Mutex::new(PoolChannelFactory::new(
            ids,
            extranonces,
            creator,
            share_per_min,
            kind,
            pool_coinbase_outputs,
        )));
        let pool = Arc::new(Mutex::new(Pool {
            downstreams: HashMap::new(),
            solution_sender,
            new_template_processed: false,
            channel_factory,
            last_prev_hash_template_id: 0,
            status_tx: status_tx.clone(),
        }));

        let cloned = pool.clone();
        let cloned2 = pool.clone();
        #[cfg(feature = "test_only_allow_unencrypted")]
        let cloned4 = pool.clone();

        info!("Starting up pool listener");
        let status_tx_clone = status_tx.clone();
        task::spawn(async move {
            if let Err(e) = Self::accept_incoming_connection(cloned, config).await {
                error!("{}", e);
            }
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting incoming connections".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });
        #[cfg(feature = "test_only_allow_unencrypted")]
        task::spawn(async {
            if let Err(e) = Self::accept_incoming_plain_connection(cloned4, config).await {
                error!("{}", e);
            }
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting incoming connections".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });

        let cloned = sender_message_received_signal.clone();
        let status_tx_clone = status_tx.clone();
        task::spawn(async move {
            if let Err(e) = Self::on_new_prev_hash(cloned2, new_prev_hash_rx, cloned).await {
                error!("{}", e);
            }
            // on_new_prev_hash shutdown
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting new prevhash".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });

        let status_tx_clone = status_tx;
        let _ = task::spawn(async move {
            if let Err(e) =
                Self::on_new_template(pool, new_template_rx, sender_message_received_signal).await
            {
                error!("{}", e);
            }
            // on_new_template shutdown
            if status_tx_clone
                .send(status::Status {
                    state: status::State::DownstreamShutdown(PoolError::ComponentShutdown(
                        "Downstream no longer accepting templates".to_string(),
                    )),
                })
                .await
                .is_err()
            {
                error!("Downstream shutdown and Status Channel dropped");
            }
        });
    }
}


#[cfg(test)]
mod test {
    use bitcoin::Transaction;
    use bitcoin::blockdata::script;
    use bitcoin::util::psbt::serialize::Serialize;
    use binary_sv2::B064K;
    use std::convert::TryInto;

    const SCRIPT_PREFIX_LEN: usize = 4;
    const PREV_OUT_LEN: usize = 38;

    // this test is used to verify the `coinbase_tx_prefix` and `coinbase_tx_suffix` tested against in
    // message generator `stratum/test/message-generator/test/pool-sri-test-extended.json`
    #[test]
    fn test_coinbase_outputs_from_config() {
        // Load config
        let config: crate::Configuration = toml::from_str(&std::fs::read_to_string("pool-config.toml").unwrap()).unwrap();

        // template from message generator
        let extranonce_len = 3;
        let coinbase_prefix: [u8; 4] = [3,1,45,0];
        let version = 536870912;
        let coinbase_tx_version = 2;
        let coinbase_tx_input_sequence = 4294967295;
        let _coinbase_tx_value_remaining: u64 = 5000000000;
        let _coinbase_tx_outputs_count = 0;
        let coinbase_tx_locktime = 0;
        let coinbase_tx_outputs = config.coinbase_outputs.iter().map(|pub_key_wrapper| {
            bitcoin::TxOut {
                value: crate::BLOCK_REWARD,
                script_pubkey: bitcoin::Script::new_p2pk(&pub_key_wrapper.pub_key),
            }
        }).collect();
        // only len supported right now
        let extranonce_len = 16;
        
        // build coinbase TX from 'job_creator::coinbase()'
        let script_prefix = coinbase_prefix.to_vec();
        let mut  bip34_bytes: Vec<u8>;
        bip34_bytes = script_prefix[1..4].to_vec();

        bip34_bytes.extend_from_slice(&vec![0; extranonce_len as usize]);
        let tx_in = bitcoin::TxIn {
            previous_output: bitcoin::OutPoint::null(),
            script_sig: bip34_bytes.into(),
            sequence: coinbase_tx_input_sequence,
            witness: vec![],
        };
        let coinbase = bitcoin::Transaction {
            version,
            lock_time: coinbase_tx_locktime,
            input: vec![tx_in],
            output: coinbase_tx_outputs,
        };

        let coinbase_tx_prefix = coinbase_tx_prefix(&coinbase, SCRIPT_PREFIX_LEN, coinbase_tx_version.clone());
        let coinbase_tx_suffix = coinbase_tx_suffix(&coinbase, 16, coinbase_tx_version);
        println!("PREFIX: {:?}", coinbase_tx_prefix);
        println!("PREFIX: {:?}", coinbase_tx_suffix);

        assert!(coinbase_tx_suffix == [255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 67, 65, 4, 70, 109, 127, 202, 229, 99, 229, 203, 9, 160, 209, 135, 11, 181, 128, 52, 72, 4, 97, 120, 121, 161, 73, 73, 207, 34, 40, 95, 27, 174, 63, 39, 103, 40, 23, 108, 60, 100, 49, 248, 238, 218, 69, 56, 220, 55, 200, 101, 226, 120, 79, 58, 158, 119, 208, 68, 243, 62, 64, 119, 151, 225, 39, 138, 172, 0, 242, 5, 42, 1, 0, 0, 0, 35, 33, 2, 52, 221, 105, 197, 108, 54, 164, 18, 48, 213, 115, 214, 138, 222, 174, 0, 48, 201, 188, 11, 242, 111, 36, 211, 225, 182, 76, 96, 77, 41, 60, 104, 172, 0, 0, 0, 0].to_vec().try_into().unwrap(),
        "coinbase_tx_suffix incorrect");


    }

    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_prefix(
        coinbase: &Transaction,
        coinbase_tx_input_script_prefix_byte_len: usize,
        _tx_version: i32,
    ) -> B064K<'static> {
        // Txs version lower or equal to 1 are not allowed in new blocks we need it only to test the
        // JobCreator against old bitcoin blocks
        let encoded = coinbase.serialize();
        let r = encoded[0..SCRIPT_PREFIX_LEN + coinbase_tx_input_script_prefix_byte_len + PREV_OUT_LEN]
            .to_vec();
        r.try_into().unwrap()
    }
    
    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_suffix(
        coinbase: &Transaction,
        extranonce_len: u8,
        _tx_version: i32,
    ) -> B064K<'static>{
        #[allow(unused_mut)]
        let mut script_prefix_len = SCRIPT_PREFIX_LEN;
        #[cfg(test)]
        if _tx_version == 1 {
            script_prefix_len = 0;
        };
        let encoded = coinbase.serialize();
        let r = encoded[4    // tx version
            + 1  // number of inputs TODO can be also 3
            + 32 // prev OutPoint
            + 4  // index
            + 1  // bytes in script TODO can be also 3
            + script_prefix_len  // bip34_bytes
            + (extranonce_len as usize)..]
            .to_vec();
        r.try_into().unwrap()
    }
}


