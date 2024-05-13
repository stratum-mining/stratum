use super::{
    error::{PoolError, PoolResult},
    status,
};
use async_channel::{Receiver, Sender};
use binary_sv2::U256;
use codec_sv2::{Frame, HandshakeRole, Responder, StandardEitherFrame, StandardSv2Frame};
use error_handling::handle_result;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey, SignatureService};
use network_helpers_sv2::noise_connection_tokio::Connection;
use nohash_hasher::BuildNoHashHasher;
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
    utils::{CoinbaseOutput as CoinbaseOutput_, Mutex},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    net::SocketAddr,
    sync::Arc,
};
use stratum_common::{
    bitcoin::{Script, TxOut},
    secp256k1,
};
use tokio::{net::TcpListener, task};
use tracing::{debug, error, info, warn};

pub mod setup_connection;
use setup_connection::SetupConnectionHandler;

pub mod message_handler;

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub fn get_coinbase_output(config: &Configuration) -> Result<Vec<TxOut>, Error> {
    let mut result = Vec::new();
    for coinbase_output_pool in &config.coinbase_outputs {
        let coinbase_output: CoinbaseOutput_ = coinbase_output_pool.try_into()?;
        let output_script: Script = coinbase_output.try_into()?;
        result.push(TxOut {
            value: 0,
            script_pubkey: output_script,
        });
    }
    match result.is_empty() {
        true => Err(Error::EmptyCoinbaseOutputs),
        _ => Ok(result),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "TEST" | "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => {
                Ok(CoinbaseOutput_ {
                    output_script_type: pool_output.clone().output_script_type,
                    output_script_value: pool_output.clone().output_script_value,
                })
            }
            _ => Err(Error::UnknownOutputScriptType),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Configuration {
    pub listen_address: String,
    pub tp_address: String,
    pub tp_authority_public_key: Option<Secp256k1PublicKey>,
    pub authority_public_key: Secp256k1PublicKey,
    pub authority_secret_key: Secp256k1SecretKey,
    pub cert_validity_sec: u64,
    pub coinbase_outputs: Vec<CoinbaseOutput>,
    pub pool_signature: String,
    #[cfg(feature = "test_only_allow_unencrypted")]
    pub test_only_listen_adress_plain: String,
}

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
    downstreams: HashMap<u32, Arc<Mutex<Downstream>>, BuildNoHashHasher<u32>>,
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
        address: SocketAddr,
    ) -> PoolResult<Arc<Mutex<Self>>> {
        let setup_connection = Arc::new(Mutex::new(SetupConnectionHandler::new()));
        let downstream_data =
            SetupConnectionHandler::setup(setup_connection, &mut receiver, &mut sender, address)
                .await?;

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
            warn!("Downstream connection dropped");
        });
        Ok(self_)
    }

    pub async fn next(self_mutex: Arc<Mutex<Self>>, mut incoming: StdFrame) -> PoolResult<()> {
        let message_type = incoming
            .get_header()
            .ok_or_else(|| PoolError::Custom(String::from("No header set")))?
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
        Self::match_send_to(self_mutex, next_message_to_send).await
    }

    #[async_recursion::async_recursion]
    async fn match_send_to(
        self_: Arc<Mutex<Self>>,
        send_to: Result<SendTo<()>, Error>,
    ) -> PoolResult<()> {
        match send_to {
            Ok(SendTo::Respond(message)) => {
                debug!("Sending to downstream: {:?}", message);
                // returning an error will send the error to the main thread,
                // and the main thread will drop the downstream from the pool
                if let &Mining::OpenMiningChannelError(_) = &message {
                    Self::send(self_.clone(), message.clone()).await?;
                    let downstream_id = self_
                        .safe_lock(|d| d.id)
                        .map_err(|e| Error::PoisonLock(e.to_string()))?;
                    return Err(PoolError::Sv2ProtocolError((
                        downstream_id,
                        message.clone(),
                    )));
                } else {
                    Self::send(self_, message.clone()).await?;
                }
            }
            Ok(SendTo::Multiple(messages)) => {
                debug!("Sending multiple messages to downstream");
                for message in messages {
                    debug!("Sending downstream message: {:?}", message);
                    Self::match_send_to(self_.clone(), Ok(message)).await?;
                }
            }
            Ok(SendTo::None(_)) => {}
            Ok(m) => {
                error!("Unexpected SendTo: {:?}", m);
                panic!();
            }
            Err(Error::UnexpectedMessage(_message_type)) => todo!(),
            Err(e) => {
                error!("Error: {:?}", e);
                todo!()
            }
        }
        Ok(())
    }

    async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::Mining<'static>,
    ) -> PoolResult<()> {
        //let message = if let Mining::NewExtendedMiningJob(job) = message {
        //    Mining::NewExtendedMiningJob(extended_job_to_non_segwit(job, 32)?)
        //} else {
        //    message
        //};
        let sv2_frame: StdFrame = PoolMessages::Mining(message).try_into()?;
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone())?;
        sender.send(sv2_frame.into()).await?;
        Ok(())
    }
}

// Verifies token for a custom job which is the signed tx_hash_list_hash by Job Declarator Server
//TODO: implement the use of this fuction in main.rs
#[allow(dead_code)]
pub fn verify_token(
    tx_hash_list_hash: U256,
    signature: secp256k1::schnorr::Signature,
    pub_key: key_utils::Secp256k1PublicKey,
) -> Result<(), secp256k1::Error> {
    let message: Vec<u8> = tx_hash_list_hash.to_vec();

    let secp = SignatureService::default();

    let is_verified = secp.verify(tx_hash_list_hash.to_vec(), signature, pub_key.0);

    // debug
    debug!("Message: {}", std::str::from_utf8(&message).unwrap());
    debug!("Verified signature {:?}", is_verified);
    is_verified
}

impl IsDownstream for Downstream {
    fn get_downstream_mining_data(&self) -> CommonDownstreamData {
        self.downstream_data
    }
}

impl IsMiningDownstream for Downstream {}

impl Pool {
    #[cfg(feature = "test_only_allow_unencrypted")]
    async fn accept_incoming_plain_connection(
        self_: Arc<Mutex<Pool>>,
        config: Configuration,
    ) -> PoolResult<()> {
        let listner = TcpListener::bind(&config.test_only_listen_adress_plain)
            .await
            .unwrap();
        let status_tx = self_.safe_lock(|s| s.status_tx.clone())?;

        info!(
            "Listening for unencrypted connection on: {}",
            config.test_only_listen_adress_plain
        );
        while let Ok((stream, _)) = listner.accept().await {
            let address = stream.peer_addr().unwrap();
            debug!("New connection from {}", address);

            let (receiver, sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
                network_helpers::plain_connection_tokio::PlainConnection::new(stream).await;

            handle_result!(
                status_tx,
                Self::accept_incoming_connection_(self_.clone(), receiver, sender, address).await
            );
        }
        Ok(())
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
            let address = stream.peer_addr().unwrap();
            debug!(
                "New connection from {:?}",
                stream.peer_addr().map_err(PoolError::Io)
            );

            let responder = Responder::from_authority_kp(
                &config.authority_public_key.into_bytes(),
                &config.authority_secret_key.into_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            );
            match responder {
                Ok(resp) => {
                    if let Ok((receiver, sender, _, _)) =
                        Connection::new(stream, HandshakeRole::Responder(resp)).await
                    {
                        handle_result!(
                            status_tx,
                            Self::accept_incoming_connection_(
                                self_.clone(),
                                receiver,
                                sender,
                                address
                            )
                            .await
                        );
                    }
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
        address: SocketAddr,
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
            address,
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
    ) -> Arc<Mutex<Self>> {
        let extranonce_len = 32;
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1 = std::ops::Range { start: 0, end: 16 };
        let range_2 = std::ops::Range {
            start: 16,
            end: extranonce_len,
        };
        let ids = Arc::new(Mutex::new(roles_logic_sv2::utils::GroupId::new()));
        let pool_coinbase_outputs = get_coinbase_output(&config);
        info!("PUB KEY: {:?}", pool_coinbase_outputs);
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
            pool_coinbase_outputs.expect("Invalid coinbase output in config"),
            config.pool_signature.clone(),
        )));
        let pool = Arc::new(Mutex::new(Pool {
            downstreams: HashMap::with_hasher(BuildNoHashHasher::default()),
            solution_sender,
            new_template_processed: false,
            channel_factory,
            last_prev_hash_template_id: 0,
            status_tx: status_tx.clone(),
        }));

        let cloned = pool.clone();
        let cloned2 = pool.clone();
        let cloned3 = pool.clone();

        #[cfg(feature = "test_only_allow_unencrypted")]
        {
            let cloned4 = pool.clone();
            let status_tx_clone_unenc = status_tx.clone();
            let config_unenc = config.clone();

            task::spawn(async move {
                if let Err(e) = Self::accept_incoming_plain_connection(cloned4, config_unenc).await
                {
                    error!("{}", e);
                }
                if status_tx_clone_unenc
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
        }

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
        task::spawn(async move {
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
        cloned3
    }

    /// This removes the downstream from the list of downstreams
    /// due to a race condition it's possible for downstreams to have been cloned right before
    /// this remove happens which will cause the cloning task to still attempt to communicate with the
    /// downstream. This is going to be rare and will won't cause any issues as the attempt to communicate
    /// will fail but continue with the next downstream.
    pub fn remove_downstream(&mut self, downstream_id: u32) {
        self.downstreams.remove(&downstream_id);
    }
}

#[cfg(test)]
mod test {
    use binary_sv2::{B0255, B064K};
    use std::convert::TryInto;

    use stratum_common::{
        bitcoin,
        bitcoin::{util::psbt::serialize::Serialize, Transaction, Witness},
    };

    // this test is used to verify the `coinbase_tx_prefix` and `coinbase_tx_suffix` values tested against in
    // message generator `stratum/test/message-generator/test/pool-sri-test-extended.json`
    #[test]
    fn test_coinbase_outputs_from_config() {
        // Load config
        let config: super::Configuration = toml::from_str(
            &std::fs::read_to_string("./config-examples/pool-config-local-tp-example.toml")
                .unwrap(),
        )
        .unwrap();
        // template from message generator test (mock TP template)
        let _extranonce_len = 3;
        let coinbase_prefix = vec![3, 76, 163, 38, 0];
        let _version = 536870912;
        let coinbase_tx_version = 2;
        let coinbase_tx_input_sequence = 4294967295;
        let _coinbase_tx_value_remaining: u64 = 625000000;
        let _coinbase_tx_outputs_count = 0;
        let coinbase_tx_locktime = 0;
        let coinbase_tx_outputs: Vec<bitcoin::TxOut> = super::get_coinbase_output(&config).unwrap();
        // extranonce len set to max_extranonce_size in `ChannelFactory::new_extended_channel()`
        let extranonce_len = 32;

        // build coinbase TX from 'job_creator::coinbase()'

        let mut bip34_bytes = get_bip_34_bytes(coinbase_prefix.try_into().unwrap());
        let script_prefix_length = bip34_bytes.len() + config.pool_signature.as_bytes().len();
        bip34_bytes.extend_from_slice(config.pool_signature.as_bytes());
        bip34_bytes.extend_from_slice(&vec![0; extranonce_len as usize]);
        let witness = match bip34_bytes.len() {
            0 => Witness::from_vec(vec![]),
            _ => Witness::from_vec(vec![vec![0; 32]]),
        };

        let tx_in = bitcoin::TxIn {
            previous_output: bitcoin::OutPoint::null(),
            script_sig: bip34_bytes.into(),
            sequence: bitcoin::Sequence(coinbase_tx_input_sequence),
            witness,
        };
        let coinbase = bitcoin::Transaction {
            version: coinbase_tx_version,
            lock_time: bitcoin::PackedLockTime(coinbase_tx_locktime),
            input: vec![tx_in],
            output: coinbase_tx_outputs,
        };

        let coinbase_tx_prefix = coinbase_tx_prefix(&coinbase, script_prefix_length);
        let coinbase_tx_suffix =
            coinbase_tx_suffix(&coinbase, extranonce_len, script_prefix_length);
        assert!(
            coinbase_tx_prefix
                == [
                    2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 56, 3, 76, 163, 38,
                    0, 83, 116, 114, 97, 116, 117, 109, 32, 118, 50, 32, 83, 82, 73, 32, 80, 111,
                    111, 108
                ]
                .to_vec()
                .try_into()
                .unwrap(),
            "coinbase_tx_prefix incorrect"
        );
        assert!(
            coinbase_tx_suffix
                == [
                    255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                    194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 1,
                    32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
                .to_vec()
                .try_into()
                .unwrap(),
            "coinbase_tx_suffix incorrect"
        );
    }

    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_prefix(coinbase: &Transaction, script_prefix_len: usize) -> B064K<'static> {
        let encoded = coinbase.serialize();
        // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the 0
        // witness
        let segwit_bytes = match script_prefix_len {
            0 => 0,
            _ => 2,
        };
        let index = 4    // tx version
            + segwit_bytes
            + 1  // number of inputs TODO can be also 3
            + 32 // prev OutPoint
            + 4  // index
            + 1  // bytes in script TODO can be also 3
            + script_prefix_len; // bip34_bytes
        let r = encoded[0..index].to_vec();
        r.try_into().unwrap()
    }

    // copied from roles-logic-sv2::job_creator
    fn coinbase_tx_suffix(
        coinbase: &Transaction,
        extranonce_len: u8,
        script_prefix_len: usize,
    ) -> B064K<'static> {
        let encoded = coinbase.serialize();
        // If script_prefix_len is not 0 we are not in a test enviornment and the coinbase have the 0
        // witness
        let segwit_bytes = match script_prefix_len {
            0 => 0,
            _ => 2,
        };
        let r = encoded[4    // tx version
        + segwit_bytes
        + 1  // number of inputs TODO can be also 3
        + 32 // prev OutPoint
        + 4  // index
        + 1  // bytes in script TODO can be also 3
        + script_prefix_len  // bip34_bytes
        + (extranonce_len as usize)..]
            .to_vec();
        r.try_into().unwrap()
    }

    fn get_bip_34_bytes(coinbase_prefix: B0255<'static>) -> Vec<u8> {
        let script_prefix = &coinbase_prefix.to_vec()[..];
        // add 1 cause 0 is push 1 2 is 1 is push 2 ecc ecc
        // add 1 cause in the len there is also the op code itself
        let bip34_len = script_prefix[0] as usize + 2;
        if bip34_len == script_prefix.len() {
            script_prefix[0..bip34_len].to_vec()
        } else {
            panic!("bip34 length does not match script prefix")
        }
    }
}
