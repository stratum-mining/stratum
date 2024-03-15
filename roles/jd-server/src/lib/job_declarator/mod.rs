pub mod message_handler;
use crate::mempool;

use super::{error::JdsError, mempool::JDsMempool, status, Configuration, EitherFrame, StdFrame};
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
    job_declaration_sv2::{DeclareMiningJob, SubmitSolutionJd},
    parsers::{JobDeclaration, PoolMessages as JdsMessages},
    utils::{Id, Mutex},
};
use secp256k1::{Keypair, Message as SecpMessage, Secp256k1};
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info};

use stratum_common::bitcoin::{
    consensus::{encode::serialize, Encodable},
    Block, Transaction, Txid,
};

// this structure wraps each transaction with the index of the position in the job declared by the
// JD-Client. the tx field can be None if the transaction is missing. In this case, the index is
// used to retrieve the transaction from the message ProvideMissingTransactionsSuccess
//#[derive(Debug)]
//struct TransactionWithIndex {
//    tx: Option<Transaction>,
//    index: u16,
//}

#[derive(Clone, Debug)]
enum TransactionState {
    Present(Txid),
    ToBeRetrievedFromNodeMempool(Txid),
    Missing,
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
        }
    }

    // This only errors that are returned are PoisonLock, Custom, MempoolError
    // this function is called in JobDeclaratorDowenstream::start(), if different errors are
    // returned, change also the error management there
    async fn retrieve_transactions_via_rpc(
        self_mutex: Arc<Mutex<JobDeclaratorDownstream>>,
    ) -> Result<(), JdsError> {
        let mut transactions_to_be_retrieved: Vec<Txid> = Vec::new();
        let mut new_transactions: Vec<Transaction> = Vec::new();
        let transactions_with_state = self_mutex
            .clone()
            .safe_lock(|a| a.declared_mining_job.clone())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?
            .1;
        for tx_with_state in transactions_with_state {
            match tx_with_state {
                TransactionState::Present(_) => continue,
                TransactionState::ToBeRetrievedFromNodeMempool(tx) => {
                    transactions_to_be_retrieved.push(tx)
                }
                TransactionState::Missing => continue,
            }
        }
        let mempool_ = self_mutex
            .safe_lock(|a| a.mempool.clone())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?;
        let client = mempool_
            .clone()
            .safe_lock(|a| a.get_client())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?
            .ok_or(JdsError::MempoolError(
                mempool::error::JdsMempoolError::NoClient,
            ))?;
        for txid in transactions_to_be_retrieved {
            let transaction = client
                .get_raw_transaction(&txid.to_string(), None)
                .await
                .map_err(|e| JdsError::MempoolError(mempool::error::JdsMempoolError::Rpc(e)))?;
            let txid = transaction.txid();
            mempool::JDsMempool::add_tx_data_to_mempool(
                mempool_.clone(),
                txid,
                Some(transaction.clone()),
            );
            new_transactions.push(transaction);
        }
        for transaction in new_transactions {
            self_mutex
                .clone()
                .safe_lock(|a| {
                    for transaction_with_state in &mut a.declared_mining_job.1 {
                        match transaction_with_state {
                            TransactionState::Present(_) => continue,
                            TransactionState::ToBeRetrievedFromNodeMempool(_) => {
                                *transaction_with_state = TransactionState::Present(transaction.txid());
                                break;
                            }
                            TransactionState::Missing => continue,
                        }
                    }
                })
                .map_err(|e| JdsError::PoisonLock(e.to_string()))?
        }
        Ok(())
    }

    fn get_block_hex(
        self_mutex: Arc<Mutex<Self>>,
        message: SubmitSolutionJd,
    ) -> Result<String, Box<JdsError>> {
        let (last_declare_, transactions_with_state, _) = self_mutex
            .safe_lock(|x| x.declared_mining_job.clone())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?;
        let mempool_ = self_mutex
            .safe_lock(|x| x.mempool.clone())
            .map_err(|e| JdsError::PoisonLock(e.to_string()))?;
        let last_declare = last_declare_.ok_or(JdsError::NoLastDeclaredJob)?;
        let mut transactions_list: Vec<Transaction> = Vec::new();
        for tx_with_state in transactions_with_state.iter().enumerate() {
            if let TransactionState::Present(txid) = tx_with_state.1 {
                let tx_ = match mempool_.safe_lock(|x| x.mempool.get(txid).cloned()) {
                    Ok(tx) => tx,
                    Err(e) => return Err(Box::new(JdsError::PoisonLock(e.to_string()))),
                };
                let tx = tx_.ok_or(JdsError::ImpossibleToReconstructBlock("Missing transactions".to_string()));
                if let Ok(Some(tx)) = tx {
                    transactions_list.push(tx);
                } else {
                    return Err(Box::new(JdsError::ImpossibleToReconstructBlock("Missing transactions".to_string())));
                }
            } else {
                return Err(Box::new(JdsError::ImpossibleToReconstructBlock(
                    "Missing transactions".to_string(),
                )));
            };
        }
        let block: Block =
            roles_logic_sv2::utils::BlockCreator::new(last_declare, transactions_list, message)
                .into();
        Ok(hex::encode(serialize(&block)))
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
                        match next_message_to_send {
                            Ok(SendTo::Respond(message)) => {
                                Self::send(self_mutex.clone(), message).await.unwrap();
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
                            Ok(SendTo::None(m)) => match m {
                                Some(JobDeclaration::SubmitSolution(message)) => {
                                    let hexdata = match JobDeclaratorDownstream::get_block_hex(
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
                            },
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
                let retrieve_transactions =
                    JobDeclaratorDownstream::retrieve_transactions_via_rpc(self_mutex.clone())
                        .await;
                match retrieve_transactions {
                    Ok(_) => (),
                    Err(error) => {
                        handle_result!(tx_status, Err(error));
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
    let kp = Keypair::from_secret_key(&secp, &secret_key);

    let message: Vec<u8> = tx_hash_list_hash.to_vec();

    let signature = secp.sign_schnorr(&SecpMessage::from_digest_slice(&message).unwrap(), &kp);

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
    ) {
        let self_ = Arc::new(Mutex::new(Self {}));
        info!("JD INITIALIZED");
        Self::accept_incoming_connection(self_, config, status_tx, mempool, new_block_sender).await;
    }
    async fn accept_incoming_connection(
        _self_: Arc<Mutex<JobDeclarator>>,
        config: Configuration,
        status_tx: crate::status::Sender,
        mempool: Arc<Mutex<JDsMempool>>,
        new_block_sender: Sender<String>,
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
