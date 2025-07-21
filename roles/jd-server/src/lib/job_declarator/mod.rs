//! # Job Declarator Server - Protocol and Downstream Handling
//!
//! This module implements the core logic of the **Job Declarator Server (JDS)**.
//!
//! Responsibilities include:
//! - Listening for downstream client connections (JDCs)
//! - Handling the Job Declaration Protocol (AllocateMiningJobToken, DeclareMiningJob, PushSolution,
//!   etc.)
//! - Tracking job state and transaction presence
//! - Managing transaction flow into the local mempool
//! - Assembling and submitting full blocks to the upstream node
//!
//! Structure:
//! - [`JobDeclarator`] handles server-level responsibilities like accepting new TCP connections.
//! - [`JobDeclaratorDownstream`] manages the per-client state and protocol interaction.
//!
//! The design is one-task-per-downstream, with communication via channels and internal
//! synchronization.

pub mod message_handler;
use super::{
    error::JdsError, mempool::JDsMempool, status, EitherFrame, JobDeclaratorServerConfig, StdFrame,
};
use async_channel::{Receiver, Sender};
use core::panic;
use error_handling::handle_result;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey, SignatureService};
use nohash_hasher::BuildNoHashHasher;
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        self,
        bitcoin::{consensus::encode::serialize, Amount, Block, Transaction, TxOut, Txid},
        codec_sv2::{
            binary_sv2::{self, B0255, U256},
            HandshakeRole, Responder,
        },
        common_messages_sv2::{
            Protocol, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
        },
        handlers::job_declaration::{ParseJobDeclarationMessagesFromDownstream, SendTo},
        job_declaration_sv2::{DeclareMiningJob, PushSolution},
        parsers_sv2::{AnyMessage as JdsMessages, JobDeclaration},
        utils::{Id, Mutex},
    },
};
use tokio::{net::TcpListener, time::Duration};
use tracing::{debug, error, info};

/// Represents whether a transaction declared in a mining job is known to the JDS mempool
/// or still missing and needs to be fetched/provided.
#[derive(Clone, Debug)]
pub enum TransactionState {
    PresentInMempool(Txid),
    Missing,
}

/// Contains transaction identifiers and full transaction data that need to be
/// added or completed in the JDS mempool.
///
/// Used internally during the job declaration lifecycle.
#[derive(Clone, Debug)]
pub struct AddTrasactionsToMempoolInner {
    pub known_transactions: Vec<Txid>,
    pub unknown_transactions: Vec<Transaction>,
}

/// Wrapper struct enabling transaction updates to be sent via a channel to the mempool task.
#[derive(Clone, Debug)]
pub struct AddTrasactionsToMempool {
    pub add_txs_to_mempool_inner: AddTrasactionsToMempoolInner,
    pub sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
}

/// Represents a single downstream connection to a JDC.
///
/// This struct tracks all state relevant to one connection, including:
/// - The declared mining job and missing transactions
/// - Mapping between tokens and job IDs
/// - Interaction with the mempool
///
/// It operates in its own async task and communicates with the rest of the system
/// via channels and locks.

#[derive(Debug)]
pub struct JobDeclaratorDownstream {
    #[allow(dead_code)]
    full_template_mode_required: bool,
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
    add_txs_to_mempool: AddTrasactionsToMempool,
}

impl JobDeclaratorDownstream {
    /// Creates a new downstream connection context.
    pub fn new(
        full_template_mode_required: bool,
        receiver: Receiver<EitherFrame>,
        sender: Sender<EitherFrame>,
        config: &JobDeclaratorServerConfig,
        mempool: Arc<Mutex<JDsMempool>>,
        sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
    ) -> Self {
        // TODO: use next variables
        let token_to_job_map = HashMap::with_hasher(BuildNoHashHasher::default());
        let tokens = Id::new();
        let add_txs_to_mempool_inner = AddTrasactionsToMempoolInner {
            known_transactions: vec![],
            unknown_transactions: vec![],
        };
        let coinbase_output = serialize(&vec![TxOut {
            value: Amount::from_sat(0),
            script_pubkey: config.coinbase_reward_scripts().script_pubkey().to_owned(),
        }]);

        Self {
            full_template_mode_required,
            receiver,
            sender,
            coinbase_output,
            token_to_job_map,
            tokens,
            public_key: *config.authority_public_key(),
            private_key: *config.authority_secret_key(),
            mempool,
            declared_mining_job: (None, Vec::new(), Vec::new()),
            add_txs_to_mempool: AddTrasactionsToMempool {
                add_txs_to_mempool_inner,
                sender_add_txs_to_mempool,
            },
        }
    }

    fn get_block_hex(
        self_mutex: Arc<Mutex<Self>>,
        message: PushSolution,
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
                transactions_list.push(tx.0);
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
                TransactionState::Missing => {
                    continue;
                }
            }
        }
        known_transactions
    }

    /// Sends a single Job Declaration message back to the downstream client.
    ///
    /// Wraps the message into a `StdFrame` and sends it through the established channel.
    pub async fn send(
        self_mutex: Arc<Mutex<Self>>,
        message: roles_logic_sv2::parsers_sv2::JobDeclaration<'static>,
    ) -> Result<(), ()> {
        let sv2_frame: StdFrame = JdsMessages::JobDeclaration(message).try_into().unwrap();
        let sender = self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
        sender.send(sv2_frame.into()).await.map_err(|_| ())?;
        Ok(())
    }

    /// Starts the message processing loop for this downstream connection.
    ///
    /// - Waits for incoming SV2 messages
    /// - Delegates message parsing to [`ParseJobDeclarationMessagesFromDownstream`]
    /// - Sends appropriate responses back to the client
    /// - Updates the JDS mempool as needed
    ///
    /// This loop runs until the client disconnects or a critical error is encountered.
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
                            ParseJobDeclarationMessagesFromDownstream::handle_message_job_declaration(
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
                        // 2. there are some unknown txids. Just before sending PMT, the JDS mempool
                        //    is triggered to fill the known txids with the full transactions. When
                        //    a PMTS arrives, just before sending a DMJS, the unknown full
                        //    transactions provided by the downstream are added to the JDS mempool
                        match next_message_to_send {
                            Ok(SendTo::Respond(m)) => {
                                match m {
                                    JobDeclaration::AllocateMiningJobToken(_) => {
                                        error!("Send unexpected message: AMJT");
                                    }
                                    JobDeclaration::AllocateMiningJobTokenSuccess(_) => {
                                        debug!("Send message: AMJTS");
                                    }
                                    JobDeclaration::DeclareMiningJob(_) => {
                                        error!("Send unexpected message: DMJ");
                                    }
                                    JobDeclaration::DeclareMiningJobError(_) => {
                                        debug!("Send nmessage: DMJE");
                                    }
                                    JobDeclaration::DeclareMiningJobSuccess(_) => {
                                        debug!("Send message: DMJS. Updating the JDS mempool.");
                                        Self::send_txs_to_mempool(self_mutex.clone()).await;
                                    }
                                    JobDeclaration::ProvideMissingTransactions(_) => {
                                        debug!("Send message: PMT. Updating the JDS mempool.");
                                        Self::send_txs_to_mempool(self_mutex.clone()).await;
                                    }
                                    JobDeclaration::ProvideMissingTransactionsSuccess(_) => {
                                        error!("Send unexpected PMTS");
                                    }
                                    JobDeclaration::PushSolution(_) => todo!(),
                                }
                                Self::send(self_mutex.clone(), m).await.unwrap();
                            }
                            Ok(SendTo::RelayNewMessage(message)) => {
                                error!("JD Server: unexpected relay new message {}", message);
                            }
                            Ok(SendTo::RelayNewMessageToRemote(remote, message)) => {
                                error!(
                                    "JD Server: unexpected relay new message to remote. Remote: {:?}, Message: {}",
                                    remote,
                                    message
                                );
                            }
                            Ok(SendTo::RelaySameMessageToRemote(remote)) => {
                                error!(
                                    "JD Server: unexpected relay same message to remote. Remote: {:?}",
                                    remote
                                );
                            }
                            Ok(SendTo::Multiple(multiple)) => {
                                error!("JD Server: unexpected multiple messages: {:?}", multiple);
                            }
                            Ok(SendTo::None(m)) => {
                                match m {
                                    Some(JobDeclaration::PushSolution(message)) => {
                                        match Self::collect_txs_in_job(self_mutex.clone()) {
                                            Ok(_) => {
                                                info!(
                                                    "All transactions in downstream job are recognized correctly by the JD Server"
                                                );
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
                                                        self_mutex.clone()
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
                                                        match JobDeclaratorDownstream::get_block_hex(
                                                            self_mutex.clone(),
                                                            message.clone(),
                                                        ) {
                                                            Ok(hexdata) => {
                                                                let _ = new_block_sender.send(hexdata).await;
                                                            },
                                                            Err(e) => {
                                                                handle_result!(
                                                                    tx_status,
                                                                    Err(*e)
                                                                );
                                                            }
                                                        };
                                                    }
                                                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                                                }
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

/// The entry point of the Job Declarator Server.
///
/// Responsible for initializing server state and accepting incoming TCP connections
/// from downstream clients (JDCs). Each client gets a dedicated [`JobDeclaratorDownstream`]
/// instance.
///
/// Responsibilities:
/// - Listening on the configured address
/// - Performing the SV2 Noise handshake
/// - Handling `SetupConnection` messages
/// - Spawning the downstream message loop
pub struct JobDeclarator {}

impl JobDeclarator {
    /// Starts the Job Declarator server.
    ///
    /// - Accepts configuration and shared components (status sender, mempool, etc.).
    /// - Initializes internal state.
    /// - Begins listening for downstream connections via
    ///   [`JobDeclarator::accept_incoming_connection`].
    pub async fn start(
        config: JobDeclaratorServerConfig,
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
        config: JobDeclaratorServerConfig,
        status_tx: crate::status::Sender,
        mempool: Arc<Mutex<JDsMempool>>,
        new_block_sender: Sender<String>,
        sender_add_txs_to_mempool: Sender<AddTrasactionsToMempoolInner>,
    ) {
        let listener = TcpListener::bind(config.listen_jd_address()).await.unwrap();

        while let Ok((stream, _)) = listener.accept().await {
            let responder = Responder::from_authority_kp(
                &config.authority_public_key().into_bytes(),
                &config.authority_secret_key().into_bytes(),
                std::time::Duration::from_secs(config.cert_validity_sec()),
            )
            .unwrap();

            let addr = stream.peer_addr();

            if let Ok((receiver, sender)) =
                Connection::new(stream, HandshakeRole::Responder(responder)).await
            {
                match receiver.recv().await {
                    Ok(EitherFrame::Sv2(mut sv2_message)) => {
                        debug!("Received SV2 message: {:?}", sv2_message);
                        let payload = sv2_message.payload();

                        if let Ok(setup_connection) =
                            binary_sv2::from_bytes::<SetupConnection>(payload)
                        {
                            let flag = setup_connection.flags;
                            let is_valid = SetupConnection::check_flags(
                                Protocol::JobDeclarationProtocol,
                                config.full_template_mode_required() as u32,
                                flag,
                            );

                            if is_valid {
                                let success_message = SetupConnectionSuccess {
                                    used_version: 2,
                                    flags: (setup_connection.flags & 1u32),
                                };
                                info!("Sending success message for proxy");
                                let sv2_frame: StdFrame = JdsMessages::Common(success_message.into())
        .try_into()
        .expect("Failed to convert setup connection response message to standard frame");

                                sender.send(sv2_frame.into()).await.unwrap();

                                let jddownstream = Arc::new(Mutex::new(
                                    JobDeclaratorDownstream::new(
                                        (setup_connection.flags & 1u32) != 0u32, /* this takes a
                                                                                  * bool instead
                                                                                  * of u32 */
                                        receiver.clone(),
                                        sender.clone(),
                                        &config,
                                        mempool.clone(),
                                        sender_add_txs_to_mempool.clone(), /* each downstream has its own sender (multi producer single consumer) */
                                    ),
                                ));

                                JobDeclaratorDownstream::start(
                                    jddownstream,
                                    status_tx.clone(),
                                    new_block_sender.clone(),
                                );
                            } else {
                                let error_message = SetupConnectionError {
                                    flags: flag,
                                    error_code: "unsupported-feature-flags"
                                        .to_string()
                                        .into_bytes()
                                        .try_into()
                                        .unwrap(),
                                };
                                info!("Sending error message for proxy");
                                let sv2_frame: StdFrame = JdsMessages::Common(error_message.into())
        .try_into()
        .expect("Failed to convert setup connection response message to standard frame");

                                sender.send(sv2_frame.into()).await.unwrap();
                            }
                        } else {
                            error!("Error parsing SetupConnection message");
                        }
                    }
                    Ok(EitherFrame::HandShake(handshake_message)) => {
                        error!(
                            "Unexpected handshake message from upstream: {:?} at {:?}",
                            handshake_message, addr
                        );
                    }
                    Err(e) => {
                        error!("Error receiving message: {:?}", e);
                    }
                }
            } else {
                error!("Cannot connect to {:?}", addr);
            }
        }
    }
}
