pub mod message_handler;
use crate::{
    error::JdsError, lib::mempool::JDsMempool, status, Configuration, EitherFrame, StdFrame,
};
use async_channel::{Receiver, Sender};
use binary_sv2::{B0255, U256};
use codec_sv2::{Frame, HandshakeRole, Responder};
use error_handling::handle_result;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers::noise_connection_tokio::Connection;
use nohash_hasher::BuildNoHashHasher;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    handlers::job_declaration::{ParseClientJobDeclarationMessages, SendTo},
    job_declaration_sv2::DeclareMiningJob,
    parsers::{JobDeclaration, PoolMessages as JdsMessages},
    utils::{merkle_root_from_path, u256_to_block_hash, Id, Mutex},
};
use secp256k1::{KeyPair, Message as SecpMessage, Secp256k1};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info};

use stratum_common::bitcoin::{
    consensus::{encode::serialize, Encodable},
    hashes::Hash,
    psbt::serialize::Deserialize,
    Block, Transaction,
};

#[derive(Debug)]
pub struct JobDeclaratorDownstream {
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    // TODO this should be computed for each new template so that fees are included
    #[allow(dead_code)]
    // TODO: use coinbase output
    coinbase_output: Vec<u8>,
    token_to_job_map: HashMap<u32, Option<u8>, BuildNoHashHasher<u32>>,
    tokens: Id,
    public_key: Secp256k1PublicKey,
    private_key: Secp256k1SecretKey,
    mempool: Arc<Mutex<JDsMempool>>,
    declared_mining_job: Option<(DeclareMiningJob<'static>, Vec<Transaction>, Vec<u16>)>,
    tx_hash_list_hash: Option<U256<'static>>,
}

impl JobDeclaratorDownstream {
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        config: &Configuration,
        mempool: Arc<Mutex<JDsMempool>>,
    ) -> Self {
        let mut coinbase_output = vec![];
        // TODO: use next variables
        let token_to_job_map = HashMap::with_hasher(BuildNoHashHasher::default());
        let tokens = Id::new();
        crate::get_coinbase_output(config).expect("Invalid coinbase output in config")[0]
            .consensus_encode(&mut coinbase_output)
            .expect("Invalid coinbase output in config");

        Self {
            receiver,
            sender,
            coinbase_output,
            token_to_job_map,
            tokens,
            public_key: config.authority_public_key.clone(),
            private_key: config.authority_secret_key.clone(),
            mempool,
            declared_mining_job: None,
            tx_hash_list_hash: None,
        }
    }

    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers::JobDeclaration<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = JdsMessages::JobDeclaration(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }
    pub fn start(
        self_mutex: Arc<Mutex<Self>>,
        tx_status: status::Sender,
        submit_solution_sender: Sender<String>,
    ) {
        let recv = self_mutex.safe_lock(|s| s.receiver.clone()).unwrap();
        tokio::spawn(async move {
            loop {
                match recv.recv().await {
                    Ok(message) => {
                        let mut frame: StdFrame = handle_result!(tx_status, message.try_into());
                        let header = frame
                            .get_header()
                            .ok_or_else(|| JdsError::Custom(String::from("No header set")));
                        let header = handle_result!(tx_status, header);
                        let message_type = header.msg_type();
                        let payload = frame.payload();
                        let next_message_to_send =
                            ParseClientJobDeclarationMessages::handle_message_job_declaration(
                                self_mutex.clone(),
                                message_type,
                                payload,
                            );
                        match next_message_to_send {
                            Ok(SendTo::Respond(message)) => {
                                Self::send(self_mutex.clone(), message).await.unwrap();
                            }
                            Ok(SendTo::None(_)) => (),
                            Ok(SendTo::RelayNewMessage(JobDeclaration::SubmitSolution(
                                message,
                            ))) => {
                                //TODO: implement logic for success or error
                                let (last_declare, mut tx_list, _) = match self_mutex
                                    .safe_lock(|x| x.declared_mining_job.take())
                                    .unwrap()
                                {
                                    Some((last_declare, tx_list, _x)) => {
                                        (last_declare, tx_list, _x)
                                    }
                                    None => {
                                        //warn!("Received solution but no job available");
                                        todo!()
                                    }
                                };
                                let coinbase_pre = last_declare.coinbase_prefix.to_vec();
                                let extranonce = message.extranonce.to_vec();
                                let coinbase_suf = last_declare.coinbase_suffix.to_vec();
                                let mut path: Vec<Vec<u8>> = vec![];
                                for tx in &tx_list {
                                    let id = tx.txid();
                                    let id = id.as_ref().to_vec();
                                    path.push(id);
                                }
                                let merkle_root = merkle_root_from_path(
                                    &coinbase_pre[..],
                                    &coinbase_suf[..],
                                    &extranonce[..],
                                    &path,
                                )
                                .expect("Invalid coinbase");
                                let merkle_root = Hash::from_inner(merkle_root.try_into().unwrap());

                                let prev_blockhash =
                                    u256_to_block_hash(message.prev_hash.into_static());
                                let header =
                                    stratum_common::bitcoin::blockdata::block::BlockHeader {
                                        version: last_declare.version as i32,
                                        prev_blockhash,
                                        merkle_root,
                                        time: message.ntime,
                                        bits: message.nbits,
                                        nonce: message.nonce,
                                    };

                                let coinbase = [coinbase_pre, extranonce, coinbase_suf].concat();
                                let coinbase = Transaction::deserialize(&coinbase[..]).unwrap();
                                tx_list.insert(0, coinbase);

                                let mut block = Block {
                                    header,
                                    txdata: tx_list.clone(),
                                };

                                block.header.merkle_root = block.compute_merkle_root().unwrap();

                                let serialized_block = serialize(&block);
                                let hexdata = hex::encode(serialized_block);

                                let _ = submit_solution_sender.send(hexdata).await;
                            }
                            Err(e) => {
                                error!("{:?}", e);
                                handle_result!(
                                    tx_status,
                                    Err(JdsError::Custom("Invalid message received".to_string()))
                                );
                                recv.close();
                                break;
                            }
                            _ => unreachable!(),
                        }
                    }
                    Err(err) => {
                        handle_result!(tx_status, Err(JdsError::ChannelRecv(err)));
                        break;
                    }
                }
            }
        });
    }
}

pub fn signed_token(
    tx_hash_list_hash: U256,
    _pub_key: &Secp256k1PublicKey,
    prv_key: &Secp256k1SecretKey,
) -> B0255<'static> {
    let secp = Secp256k1::signing_only();

    // Create the SecretKey and PublicKey instances
    let secret_key = prv_key.0;
    let kp = KeyPair::from_secret_key(&secp, &secret_key);

    let message: Vec<u8> = tx_hash_list_hash.to_vec();

    let signature = secp.sign_schnorr(&SecpMessage::from_slice(&message).unwrap(), &kp);

    // Sign message
    signature.as_ref().to_vec().try_into().unwrap()
}

fn _get_random_token() -> B0255<'static> {
    let inner: [u8; 32] = rand::random();
    inner.to_vec().try_into().unwrap()
}

pub struct JobDeclarator {}

impl JobDeclarator {
    pub async fn start(
        config: Configuration,
        status_tx: crate::status::Sender,
        mempool: Arc<Mutex<JDsMempool>>,
        sender: Sender<String>,
    ) {
        let self_ = Arc::new(Mutex::new(Self {}));
        info!("JD INITIALIZED");
        Self::accept_incoming_connection(self_, config, status_tx, mempool, sender).await;
    }
    async fn accept_incoming_connection(
        _self_: Arc<Mutex<JobDeclarator>>,
        config: Configuration,
        status_tx: crate::status::Sender,
        mempool: Arc<Mutex<JDsMempool>>,
        submit_solution_sender: Sender<String>,
    ) {
        let listner = TcpListener::bind(&config.listen_jd_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                &config.authority_public_key.clone().into_bytes(),
                &config.authority_secret_key.clone().into_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec),
            )
            .unwrap();
            let addr = stream.peer_addr();

            if let Ok((receiver, sender, _, _)) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await
            {
                let setup_message_from_proxy_jd = receiver.recv().await.unwrap();
                info!(
                    "Setup connection message from proxy: {:?}",
                    setup_message_from_proxy_jd
                );

                let setup_connection_success_to_proxy = SetupConnectionSuccess {
                    used_version: 2,
                    // Setup flags for async_mining_allowed
                    flags: 0b_0000_0000_0000_0000_0000_0000_0000_0001,
                };
                let sv2_frame: StdFrame =
                    JdsMessages::Common(setup_connection_success_to_proxy.into())
                        .try_into()
                        .unwrap();
                let sv2_frame = sv2_frame.into();
                info!("Sending success message for proxy");
                sender.send(sv2_frame).await.unwrap();

                let jddownstream = Arc::new(Mutex::new(JobDeclaratorDownstream::new(
                    receiver.clone(),
                    sender.clone(),
                    &config,
                    mempool.clone(),
                )));

                JobDeclaratorDownstream::start(
                    jddownstream,
                    status_tx.clone(),
                    submit_solution_sender.clone(),
                );
            } else {
                error!("Can not connect {:?}", addr);
            }
        }
    }
}
