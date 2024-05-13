pub mod message_handler;
use super::{error::JdsError, mempool::JDsMempool, status, Configuration, EitherFrame, StdFrame};
use async_channel::{Receiver, Sender};
use binary_sv2::{B0255, U256};
use codec_sv2::{Frame, HandshakeRole, Responder};
use error_handling::handle_result;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey, SignatureService};
use network_helpers_sv2::noise_connection_tokio::Connection;
use nohash_hasher::BuildNoHashHasher;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess,
    handlers::job_declaration::{ParseClientJobDeclarationMessages, SendTo},
    job_declaration_sv2::{DeclareMiningJob, SubmitSolutionJd},
    parsers::{JobDeclaration, PoolMessages as JdsMessages},
    utils::{Id, Mutex},
};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::{net::TcpListener, time::Duration};
use tracing::{debug, error, info};

use stratum_common::bitcoin::{
    consensus::{encode::serialize, Encodable},
    Block, Transaction, Txid,
};

#[derive(Clone, Debug)]
pub enum TransactionState {
    PresentInMempool(Txid),
    Missing,
}

#[derive(Clone, Debug)]
pub struct AddTrasactionsToMempoolInner {
    pub known_transactions: Vec<Txid>,
    pub unknown_transactions: Vec<Transaction>,
}

// TODO implement send method that sends the inner via the sender
#[derive(Clone, Debug)]
pub struct AddTrasactionsToMempool {
    pub add_txs_to_mempool_inner: AddTrasactionsToMempoolInner,
    pub sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
}

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
    // Vec<u16> is the vector of missing transactions
    declared_mining_job: (
        Option<DeclareMiningJob<'static>>,
        Vec<TransactionState>,
        Vec<u16>,
    ),
    tx_hash_list_hash: Option<U256<'static>>,
    add_txs_to_mempool: AddTrasactionsToMempool,
}

impl JobDeclaratorDownstream {
    pub fn new(
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        config: &Configuration,
        mempool: Arc<Mutex<JDsMempool>>,
        sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
    ) -> Self {
        let mut coinbase_output = vec![];
        // TODO: use next variables
        let token_to_job_map = HashMap::with_hasher(BuildNoHashHasher::default());
        let tokens = Id::new();
        let add_txs_to_mempool_inner = AddTrasactionsToMempoolInner {
            known_transactions: vec![],
            unknown_transactions: vec![],
        };
        super::get_coinbase_output(config).expect("Invalid coinbase output in config")[0]
            .consensus_encode(&mut coinbase_output)
            .expect("Invalid coinbase output in config");

        Self {
            receiver,
            sender,
            coinbase_output,
            token_to_job_map,
            tokens,
            public_key: config.authority_public_key,
            private_key: config.authority_secret_key,
            mempool,
            declared_mining_job: (None, Vec::new(), Vec::new()),
            tx_hash_list_hash: None,
            add_txs_to_mempool: AddTrasactionsToMempool {
                add_txs_to_mempool_inner,
                sender_add_txs_to_mempool,
            },
        }
    }

    fn get_block_hex(
        self_mutex: Arc<Mutex<Self>>,
        message: SubmitSolutionJd,
    ) -> Result<String, Box<JdsError>> {
        let (last_declare_, _, _) = self_mutex
            .clone()
            .safe_lock(|x| x.declared_mining_job.clone())
            .map_err(|e| Box::new(JdsError::PoisonLock(e.to_string())))?;
        let last_declare = last_declare_.ok_or(Box::new(JdsError::NoLastDeclaredJob))?;
        let transactions_list = Self::collect_txs_in_job(self_mutex)?;
        let block: Block =
            roles_logic_sv2::utils::BlockCreator::new(last_declare, transactions_list, message)
                .into();
        Ok(hex::encode(serialize(&block)))
    }

    fn collect_txs_in_job(self_mutex: Arc<Mutex<Self>>) -> Result<Vec<Transaction>, Box<JdsError>> {
        let (_, transactions_with_state, _) = self_mutex
            .clone()
            .safe_lock(|x| x.declared_mining_job.clone())
            .map_err(|e| Box::new(JdsError::PoisonLock(e.to_string())))?;
        let mempool = self_mutex
            .safe_lock(|x| x.mempool.clone())
            .map_err(|e| Box::new(JdsError::PoisonLock(e.to_string())))?;
        let mut transactions_list: Vec<Transaction> = Vec::new();
        for tx_with_state in transactions_with_state.iter().enumerate() {
            if let TransactionState::PresentInMempool(txid) = tx_with_state.1 {
                let tx = mempool
                    .safe_lock(|x| x.mempool.get(txid).cloned())
                    .map_err(|e| JdsError::PoisonLock(e.to_string()))?
                    .ok_or(Box::new(JdsError::ImpossibleToReconstructBlock(
                        "Txid not found in jds mempool".to_string(),
                    )))?
                    .ok_or(Box::new(JdsError::ImpossibleToReconstructBlock(
                        "Txid found in jds mempool but transactions not present".to_string(),
                    )))?;
                transactions_list.push(tx);
            } else {
                return Err(Box::new(JdsError::ImpossibleToReconstructBlock(
                    "Unknown transaction".to_string(),
                )));
            };
        }
        Ok(transactions_list)
    }

    async fn send_txs_to_mempool(self_mutex: Arc<Mutex<Self>>) {
        let add_txs_to_mempool = self_mutex
            .safe_lock(|a| a.add_txs_to_mempool.clone())
            .unwrap();
        let sender_add_txs_to_mempool = add_txs_to_mempool.sender_add_txs_to_mempool;
        let add_txs_to_mempool_inner = add_txs_to_mempool.add_txs_to_mempool_inner;
        let _ = sender_add_txs_to_mempool
            .send(add_txs_to_mempool_inner)
            .await;
        // the trasnactions sent to the mempool can be freed
        let _ = self_mutex.safe_lock(|a| {
            a.add_txs_to_mempool.add_txs_to_mempool_inner = AddTrasactionsToMempoolInner {
                known_transactions: vec![],
                unknown_transactions: vec![],
            };
        });
    }

    fn get_transactions_in_job(self_mutex: Arc<Mutex<Self>>) -> Vec<Txid> {
        let mut known_transactions: Vec<Txid> = Vec::new();
        let job_transactions = self_mutex
            .safe_lock(|a| a.declared_mining_job.1.clone())
            .unwrap();
        for transaction in job_transactions {
            match transaction {
                TransactionState::PresentInMempool(txid) => known_transactions.push(txid),
                TransactionState::Missing => continue,
            };
        }
        known_transactions
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
        new_block_sender: Sender<String>,
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
                        // How works the txs recognition and txs storing in JDS mempool
                        // when a DMJ arrives, the JDS compares the received transactions with the
                        // ids in the the JDS mempool. Then there are two scenarios
                        // 1. the JDS recognizes all the transactions. Then, just before a DMJS is
                        //    sent, the JDS mempool is triggered to fill in the JDS mempool the id
                        //    of declared job with the full transaction (with send_tx_to_mempool
                        //    method(), that eventually will ask the transactions to a bitcoin node
                        //    via RPC)
                        // 2. there are some unknown txids. Just before sending PMT, the JDS
                        //    mempool is triggered to fill the known txids with the full
                        //    transactions. When a PMTS arrives, just before sending a DMJS, the
                        //    unknown full transactions provided by the downstream are added to the
                        //    JDS mempool
                        match next_message_to_send {
                            Ok(SendTo::Respond(m)) => {
                                match m {
                                    JobDeclaration::AllocateMiningJobToken(_) => {
                                        error!("Send unexpected message: AMJT")
                                    }
                                    JobDeclaration::AllocateMiningJobTokenSuccess(_) => {
                                        debug!("Send message: AMJTS")
                                    }
                                    JobDeclaration::DeclareMiningJob(_) => {
                                        error!("Send unexpected message: DMJ");
                                    }
                                    JobDeclaration::DeclareMiningJobError(_) => {
                                        debug!("Send nmessage: DMJE")
                                    }
                                    JobDeclaration::DeclareMiningJobSuccess(_) => {
                                        debug!("Send message: DMJS. Updating the JDS mempool.");
                                        Self::send_txs_to_mempool(self_mutex.clone()).await;
                                    }
                                    JobDeclaration::IdentifyTransactions(_) => {
                                        debug!("Send  message: IT")
                                    }
                                    JobDeclaration::IdentifyTransactionsSuccess(_) => {
                                        error!("Send unexpected message: ITS")
                                    }
                                    JobDeclaration::ProvideMissingTransactions(_) => {
                                        debug!("Send message: PMT. Updating the JDS mempool.");
                                        Self::send_txs_to_mempool(self_mutex.clone()).await;
                                    }
                                    JobDeclaration::ProvideMissingTransactionsSuccess(_) => {
                                        error!("Send unexpected PMTS");
                                    }
                                    JobDeclaration::SubmitSolution(_) => todo!(),
                                }
                                Self::send(self_mutex.clone(), m).await.unwrap();
                            }
                            Ok(SendTo::RelayNewMessage(message)) => {
                                error!("JD Server: unexpected relay new message {:?}", message);
                            }
                            Ok(SendTo::RelayNewMessageToRemote(remote, message)) => {
                                error!("JD Server: unexpected relay new message to remote. Remote: {:?}, Message: {:?}", remote, message);
                            }
                            Ok(SendTo::RelaySameMessageToRemote(remote)) => {
                                error!("JD Server: unexpected relay same message to remote. Remote: {:?}", remote);
                            }
                            Ok(SendTo::Multiple(multiple)) => {
                                error!("JD Server: unexpected multiple messages: {:?}", multiple);
                            }
                            Ok(SendTo::None(m)) => {
                                match m {
                                    Some(JobDeclaration::SubmitSolution(message)) => {
                                        match Self::collect_txs_in_job(self_mutex.clone()) {
                                            Ok(_) => {
                                                info!("All transactions in downstream job are recognized correctly by the JD Server");
                                                let hexdata =
                                                    match JobDeclaratorDownstream::get_block_hex(
                                                        self_mutex.clone(),
                                                        message,
                                                    ) {
                                                        Ok(inner) => inner,
                                                        Err(e) => {
                                                            error!(
                                                                "Received solution but encountered error: {:?}",
                                                                e
                                                            );
                                                            recv.close();
                                                            //TODO should we brake it?
                                                            break;
                                                        }
                                                    };
                                                let _ = new_block_sender.send(hexdata).await;
                                            }
                                            Err(error) => {
                                                error!("Missing transactions: {:?}", error);
                                                // TODO print here the ip of the downstream
                                                let known_transactions =
                                                    JobDeclaratorDownstream::get_transactions_in_job(
                                                        self_mutex.clone(),
                                                    );
                                                let retrieve_transactions =
                                                    AddTrasactionsToMempoolInner {
                                                        known_transactions,
                                                        unknown_transactions: Vec::new(),
                                                    };
                                                let mempool = self_mutex
                                                    .clone()
                                                    .safe_lock(|a| a.mempool.clone())
                                                    .unwrap();
                                                tokio::select! {
                                                    _ = JDsMempool::add_tx_data_to_mempool(mempool, retrieve_transactions) => {
                                                        let hexdata = match JobDeclaratorDownstream::get_block_hex(
                                                            self_mutex.clone(),
                                                            message.clone(),
                                                        ) {
                                                            Ok(inner) => inner,
                                                            Err(e) => {
                                                                error!(
                                                                    "Error retrieving transactions: {:?}",
                                                                    e
                                                                );
                                                                recv.close();
                                                                //TODO should we brake it?
                                                                break;
                                                            }
                                                        };
                                                        let _ = new_block_sender.send(hexdata).await;
                                                    }
                                                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                                                };
                                            }
                                        };
                                    }
                                    Some(JobDeclaration::DeclareMiningJob(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::DeclareMiningJobSuccess(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::DeclareMiningJobError(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::IdentifyTransactions(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::IdentifyTransactionsSuccess(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::AllocateMiningJobToken(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::AllocateMiningJobTokenSuccess(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::ProvideMissingTransactions(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    Some(JobDeclaration::ProvideMissingTransactionsSuccess(_)) => {
                                        error!("JD Server received an unexpected message {:?}", m);
                                    }
                                    None => (),
                                }
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
    let secp = SignatureService::default();

    let signature = secp.sign(tx_hash_list_hash.to_vec(), prv_key.0);

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
        new_block_sender: Sender<String>,
        sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
    ) {
        let self_ = Arc::new(Mutex::new(Self {}));
        info!("JD INITIALIZED");
        Self::accept_incoming_connection(
            self_,
            config,
            status_tx,
            mempool,
            new_block_sender,
            sender_add_txs_to_mempool,
        )
        .await;
    }
    async fn accept_incoming_connection(
        _self_: Arc<Mutex<JobDeclarator>>,
        config: Configuration,
        status_tx: crate::status::Sender,
        mempool: Arc<Mutex<JDsMempool>>,
        new_block_sender: Sender<String>,
        sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
    ) {
        let listner = TcpListener::bind(&config.listen_jd_address).await.unwrap();
        while let Ok((stream, _)) = listner.accept().await {
            let responder = Responder::from_authority_kp(
                &config.authority_public_key.into_bytes(),
                &config.authority_secret_key.into_bytes(),
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
                    // each downstream has its own sender (multi producer single consumer)
                    sender_add_txs_to_mempool.clone(),
                )));

                JobDeclaratorDownstream::start(
                    jddownstream,
                    status_tx.clone(),
                    new_block_sender.clone(),
                );
            } else {
                error!("Can not connect {:?}", addr);
            }
        }
    }
}
