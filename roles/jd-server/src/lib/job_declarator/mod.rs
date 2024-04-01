pub mod message_handler;
use super::{downstream::JobDeclaratorDownstream, mempool::JDsMempool, Configuration, StdFrame};
use async_channel::Sender;
use binary_sv2::{B0255, U256};
use codec_sv2::{HandshakeRole, Responder};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionSuccess, parsers::PoolMessages as JdsMessages, utils::Mutex,
};
use secp256k1::{Keypair, Message as SecpMessage, Secp256k1};
use std::{convert::TryInto, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info};

use stratum_common::bitcoin::{Transaction, Txid};

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
