//! ## Template Receiver (JDC)
//! Contains the logic required for the Job Declarator Client (JDC) to connect to and communicate
//! with a Template Provider (TP).
//!
//! This includes establishing a secure connection, sending and receiving SV2 Template Distribution
//! protocol messages, handling template-related events, and coordinating with the job declarator
//! and downstream subsystem.
use super::{job_declarator::JobDeclarator, status, PoolChangerTrigger};
use async_channel::{Receiver, Sender};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use setup_connection::SetupConnectionHandler;
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        self,
        bitcoin::{
            absolute::LockTime,
            blockdata::witness::Witness,
            consensus::{deserialize, serialize, Encodable},
            script::ScriptBuf,
            transaction::{OutPoint, Transaction, Version},
            Amount, Sequence, TxIn, TxOut,
        },
        codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame},
        handlers::{template_distribution::ParseTemplateDistributionMessagesFromServer, SendTo_},
        job_declaration_sv2::AllocateMiningJobTokenSuccess,
        parsers_sv2::{AnyMessage, TemplateDistribution},
        template_distribution_sv2::{
            CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, SubmitSolution,
        },
        utils::Mutex,
    },
};
use tokio::task::AbortHandle;
use tracing::{error, info, warn};

mod message_handler;
mod setup_connection;

pub type SendTo = SendTo_<roles_logic_sv2::parsers_sv2::TemplateDistribution<'static>, ()>;
pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

/// Represents a template receiver client
pub struct TemplateRx {
    // Receiver channel for incoming messages from the Template Provider.
    receiver: Receiver<EitherFrame>,
    // Sender channel for sending messages to the Template Provider.
    sender: Sender<EitherFrame>,
    // Sender for communicating status updates back to the main status loop
    // for error handling and state management.
    tx_status: status::Sender,
    // Present when connected to a pool, absent in solo mining mode.
    jd: Option<Arc<Mutex<super::job_declarator::JobDeclarator>>>,
    // used for sending template and job information to the downstream.
    down: Arc<Mutex<super::downstream::DownstreamMiningNode>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    // Stores the last received `NewTemplate` message.
    new_template_message: Option<NewTemplate<'static>>,
    // Trigger mechanism to detect unresponsive upstream behavior and initiate a pool change.
    pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
    // The encoded miner's coinbase output(s) from the configuration.
    miner_coinbase_output: Vec<u8>,
}

impl TemplateRx {
    // The connect method connects to the Template Provider over TCP, performs the SV2 setup
    // connection handshake, and starts background tasks for handling incoming template messages
    // and forwarding miner solutions.
    //
    // This is the entry point for establishing communication with the Template Provider.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
        address: SocketAddr,
        solution_receiver: Receiver<SubmitSolution<'static>>,
        tx_status: status::Sender,
        jd: Option<Arc<Mutex<super::job_declarator::JobDeclarator>>>,
        down: Arc<Mutex<super::downstream::DownstreamMiningNode>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
        pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
        miner_coinbase_outputs: Vec<TxOut>,
        authority_public_key: Option<Secp256k1PublicKey>,
    ) {
        let mut encoded_outputs = vec![];
        // If in solo mining mode (jd is None), encode only the first coinbase output
        // as per JDS behavior. Otherwise, encode all provided outputs.
        if jd.is_none() {
            miner_coinbase_outputs[0]
                .consensus_encode(&mut encoded_outputs)
                .expect("Invalid coinbase output in config");
        } else {
            miner_coinbase_outputs
                .consensus_encode(&mut encoded_outputs)
                .expect("Invalid coinbase output in config");
        }
        // Establish a TCP connection to the Template Provider address.
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();

        let initiator = match authority_public_key {
            Some(pub_key) => Initiator::from_raw_k(pub_key.into_bytes()),
            None => Initiator::without_pk(),
        }
        .unwrap();
        let (mut receiver, mut sender) =
            Connection::new(stream, HandshakeRole::Initiator(initiator))
                .await
                .unwrap();

        info!("Template Receiver try to set up connection");
        // Perform the SV2 setup connection handshake with the Template Provider.
        SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
            .await
            .unwrap();
        info!("Template Receiver connection set up");

        let self_mutex = Arc::new(Mutex::new(Self {
            receiver: receiver.clone(),
            sender: sender.clone(),
            tx_status,
            jd,
            down,
            task_collector: task_collector.clone(),
            new_template_message: None,
            pool_chaneger_trigger,
            miner_coinbase_output: encoded_outputs,
        }));

        // Spawn a task to handle incoming block solutions from the miner and forward them
        // to the Template Provider
        let task = tokio::task::spawn(Self::on_new_solution(self_mutex.clone(), solution_receiver));
        task_collector
            .safe_lock(|c| c.push(task.abort_handle()))
            .unwrap();

        // Start the main task for receiving and processing template-related messages
        // from the Template Provider.
        Self::start_templates(self_mutex);
    }

    /// This method is used to send message to template provider.
    pub async fn send(self_: &Arc<Mutex<Self>>, sv2_frame: StdFrame) {
        let either_frame = sv2_frame.into();
        let sender_to_tp = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender_to_tp.send(either_frame).await {
            Ok(_) => (),
            Err(e) => panic!("{e:?}"),
        }
    }

    /// Sends a `CoinbaseOutputConstraints` message to the Template Provider.
    ///
    /// This informs the TP about the maximum size and sigops allowed in the miner's
    /// additional coinbase output data.
    pub async fn send_coinbase_output_constraints(
        self_mutex: &Arc<Mutex<Self>>,
        size: u32,
        sigops: u16,
    ) {
        let coinbase_output_data_size = AnyMessage::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
                coinbase_output_max_additional_size: size,
                coinbase_output_max_additional_sigops: sigops,
            }),
        );
        let frame: StdFrame = coinbase_output_data_size.try_into().unwrap();
        Self::send(self_mutex, frame).await;
    }

    /// Sends a `RequestTransactionData` message to the Template Provider.
    ///
    /// This requests the full transaction data for a template identified by its ID.
    pub async fn send_tx_data_request(
        self_mutex: &Arc<Mutex<Self>>,
        new_template: NewTemplate<'static>,
    ) {
        let tx_data_request = AnyMessage::TemplateDistribution(
            TemplateDistribution::RequestTransactionData(RequestTransactionData {
                template_id: new_template.template_id,
            }),
        );
        let frame: StdFrame = tx_data_request.try_into().unwrap();
        Self::send(self_mutex, frame).await;
    }

    /// Retrieves the last allocated mining job token.
    ///
    /// If the JDC is connected to a pool, it fetches the token from the `JobDeclarator`.
    /// In solo mining mode, it generates a dummy token with constraints derived from
    /// the miner's configured coinbase output.
    async fn get_last_token(
        jd: Option<Arc<Mutex<JobDeclarator>>>,
        miner_coinbase_output: &[u8],
    ) -> AllocateMiningJobTokenSuccess<'static> {
        if let Some(jd) = jd {
            JobDeclarator::get_last_token(&jd).await
        } else {
            // This is when JDC is doing solo mining

            AllocateMiningJobTokenSuccess {
                request_id: 0,
                mining_job_token: vec![0; 32].try_into().unwrap(),
                coinbase_outputs: miner_coinbase_output.to_vec().try_into().unwrap(),
            }
        }
    }

    /// Contains the core logic for the Template Receiver's main operational loop.
    ///
    /// This function is responsible for:
    /// 1. Sending initial `CoinbaseOutputConstraints` to the Template Provider.
    /// 2. Continuously receiving and processing messages from the Template Provider.
    /// 3. Handling different Template Distribution messages (`NewTemplate`, `SetNewPrevHash`,
    ///    `RequestTransactionDataSuccess`, `RequestTransactionDataError`).
    /// 4. Requesting transaction data for new templates.
    /// 5. Coordinating the delivery of template and job information to the `JobDeclarator` (when
    ///    connected to a pool) and the `DownstreamMiningNode`.
    /// 6. Utilizing the `IS_NEW_TEMPLATE_HANDLED` global atomic for synchronization between the
    ///    template receiver and downstream when processing `NewTemplate` and `SetNewPrevHash`.
    ///
    /// FIX ME: Remove dependence from other modules in this. This gonna help in
    /// removing sequential component spawning.
    pub fn start_templates(self_mutex: Arc<Mutex<Self>>) {
        let jd = self_mutex.safe_lock(|s| s.jd.clone()).unwrap();
        let down = self_mutex.safe_lock(|s| s.down.clone()).unwrap();
        let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
        let mut coinbase_output_constraints_sent = false;
        let mut last_token = None;
        let miner_coinbase_output = self_mutex
            .safe_lock(|s| s.miner_coinbase_output.clone())
            .unwrap();

        // Spawn the main task for handling incoming template messages.
        let main_task = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                // Send CoinbaseOutputConstraints to TP
                loop {
                    // Retrieve the last allocated mining job token if not already available.
                    if last_token.is_none() {
                        let jd = self_mutex.safe_lock(|s| s.jd.clone()).unwrap();
                        last_token =
                            Some(Self::get_last_token(jd, &miner_coinbase_output[..]).await);
                    }
                    // Send CoinbaseOutputConstraints to the Template Provider if not already sent.
                    if !coinbase_output_constraints_sent {
                        coinbase_output_constraints_sent = true;

                        let jds_coinbase_outputs =
                            last_token.clone().unwrap().coinbase_outputs.to_vec();
                        let deserialized_jds_coinbase_outputs: Vec<TxOut> =
                            deserialize(&jds_coinbase_outputs).expect("Invalid coinbase output");

                        let coinbase_output_max_additional_size: usize =
                            deserialized_jds_coinbase_outputs
                                .iter()
                                .map(|o| o.size())
                                .sum();

                        // create a dummy coinbase transaction with the empty output
                        // this is used to calculate the sigops of the coinbase output
                        let dummy_coinbase = Transaction {
                            version: Version::TWO,
                            lock_time: LockTime::ZERO,
                            input: vec![TxIn {
                                previous_output: OutPoint::null(),
                                script_sig: ScriptBuf::new(),
                                sequence: Sequence::MAX,
                                witness: Witness::from(vec![vec![0; 32]]),
                            }],
                            output: deserialized_jds_coinbase_outputs,
                        };

                        let coinbase_output_max_additional_sigops =
                            dummy_coinbase.total_sigop_cost(|_| None) as u16;

                        Self::send_coinbase_output_constraints(
                            &self_mutex,
                            coinbase_output_max_additional_size as u32,
                            coinbase_output_max_additional_sigops,
                        )
                        .await;
                    }

                    // Receive Templates and SetPrevHash from TP to send to JD
                    let receiver = self_mutex
                        .clone()
                        .safe_lock(|s| s.receiver.clone())
                        .unwrap();
                    let received = handle_result!(tx_status.clone(), receiver.recv().await);
                    let mut frame: StdFrame =
                        handle_result!(tx_status.clone(), received.try_into());
                    let message_type = frame.get_header().unwrap().msg_type();
                    let payload = frame.payload();

                    // Process the received message using the template distribution message handler
                    let next_message_to_send =
                        ParseTemplateDistributionMessagesFromServer::handle_message_template_distribution(
                            self_mutex.clone(),
                            message_type,
                            payload,
                        );
                    match next_message_to_send {
                        Ok(SendTo::None(m)) => {
                            match m {
                                // Send the new template along with the token to the JD so that JD
                                // can declare the mining job
                                Some(TemplateDistribution::NewTemplate(m)) => {
                                    // Set the global flag to false (Release ordering) to signal
                                    // that a new template is being handled by the downstream.
                                    super::IS_NEW_TEMPLATE_HANDLED
                                        .store(false, std::sync::atomic::Ordering::Release);
                                    // Request transaction data for the new template.
                                    Self::send_tx_data_request(&self_mutex, m.clone()).await;
                                    self_mutex
                                        .safe_lock(|t| t.new_template_message = Some(m.clone()))
                                        .unwrap();
                                    // Get the pool's coinbase output from the last token.
                                    let token = last_token.clone().unwrap();
                                    let pool_outputs = token.coinbase_outputs.to_vec();

                                    // Notify the downstream mining node about the new template.
                                    super::downstream::DownstreamMiningNode::on_new_template(
                                        &down,
                                        m.clone(),
                                        &pool_outputs[..],
                                    )
                                    .await
                                    .unwrap();
                                }
                                // Handle SetNewPrevHash messages.
                                Some(TemplateDistribution::SetNewPrevHash(m)) => {
                                    info!("Received SetNewPrevHash, waiting for IS_NEW_TEMPLATE_HANDLED");
                                    // Wait until the IS_NEW_TEMPLATE_HANDLED flag is true,
                                    // indicating the downstream has finished processing the
                                    // previous NewTemplate.
                                    while !super::IS_NEW_TEMPLATE_HANDLED
                                        .load(std::sync::atomic::Ordering::Acquire)
                                    {
                                        tokio::task::yield_now().await;
                                    }
                                    info!("IS_NEW_TEMPLATE_HANDLED ok");
                                    // If connected to a pool, notify the Job Declarator about the
                                    // new prev hash.
                                    if let Some(jd) = jd.as_ref() {
                                        super::job_declarator::JobDeclarator::on_set_new_prev_hash(
                                            jd.clone(),
                                            m.clone(),
                                        );
                                    }
                                    // Notify the downstream mining node about the new prev hash.
                                    super::downstream::DownstreamMiningNode::on_set_new_prev_hash(
                                        &down, m,
                                    )
                                    .await
                                    .unwrap();
                                }
                                // Handle RequestTransactionDataSuccess messages.
                                Some(TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                                    // safe to unwrap because this message is received after the new
                                    // template message
                                    let transactions_data = m.transaction_list;
                                    let excess_data = m.excess_data;

                                    // Retrieve the stored NewTemplate message (safe to unwrap as
                                    // this message follows a NewTemplate).
                                    let m = self_mutex
                                        .safe_lock(|t| t.new_template_message.clone())
                                        .unwrap()
                                        .unwrap();

                                    // Retrieve the last token and reset the stored token.
                                    let token = last_token.unwrap();
                                    last_token = None;

                                    // Extract mining token and pool coinbase output from the token.
                                    let mining_token = token.mining_job_token.to_vec();
                                    let pool_coinbase_outputs = token.coinbase_outputs.to_vec();

                                    let mut deserialized_outputs: Vec<TxOut> =
                                        deserialize(&pool_coinbase_outputs).unwrap();

                                    // we know the first output is where the template revenue must
                                    // be allocated
                                    deserialized_outputs[0].value =
                                        Amount::from_sat(m.coinbase_tx_value_remaining);

                                    let reserialized_outputs = serialize(&deserialized_outputs);

                                    // If connected to a pool, notify the Job Declarator with the
                                    // complete template information (including transactions).
                                    if let Some(jd) = jd.as_ref() {
                                        super::job_declarator::JobDeclarator::on_new_template(
                                            jd,
                                            m.clone(),
                                            mining_token,
                                            transactions_data,
                                            excess_data,
                                            reserialized_outputs,
                                        )
                                        .await;
                                    }
                                }
                                Some(TemplateDistribution::RequestTransactionDataError(_)) => {
                                    warn!("The prev_hash of the template requested to Template Provider no longer points to the latest tip. Continuing work on the updated template.")
                                }
                                _ => {
                                    error!("{:?}", frame);
                                    error!("{:?}", frame.payload());
                                    error!("{:?}", frame.get_header());
                                    std::process::exit(1);
                                }
                            }
                        }
                        Ok(m) => {
                            error!("{:?}", m);
                            error!("{:?}", frame);
                            error!("{:?}", frame.payload());
                            error!("{:?}", frame.get_header());
                            std::process::exit(1);
                        }
                        Err(e) => {
                            error!("{:?}", e);
                            error!("{:?}", frame);
                            error!("{:?}", frame.payload());
                            error!("{:?}", frame.get_header());
                            std::process::exit(1);
                        }
                    }
                }
            })
        };
        self_mutex
            .safe_lock(|s| {
                s.task_collector
                    .safe_lock(|c| c.push(main_task.abort_handle()))
                    .unwrap()
            })
            .unwrap();
    }

    /// Handles incoming `SubmitSolution` messages from the miner.
    ///
    /// This method continuously receives solutions from the provided receiver channel
    /// and forwards them as `SubmitSolution` messages to the Template Provider.
    async fn on_new_solution(self_: Arc<Mutex<Self>>, rx: Receiver<SubmitSolution<'static>>) {
        while let Ok(solution) = rx.recv().await {
            let sv2_frame: StdFrame =
                AnyMessage::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into()
                    .expect("Failed to convert solution to sv2 frame!");
            Self::send(&self_, sv2_frame).await
        }
    }
}
