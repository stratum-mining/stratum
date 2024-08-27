use super::{job_declarator::JobDeclarator, status, PoolChangerTrigger};
use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame};
use error_handling::handle_result;
use key_utils::Secp256k1PublicKey;
use network_helpers_sv2::noise_connection_tokio::Connection;
use roles_logic_sv2::{
    handlers::{template_distribution::ParseServerTemplateDistributionMessages, SendTo_},
    job_declaration_sv2::AllocateMiningJobTokenSuccess,
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, SubmitSolution,
    },
    utils::Mutex,
};
use setup_connection::SetupConnectionHandler;
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use stratum_common::bitcoin::{consensus::Encodable, TxOut};
use tokio::task::AbortHandle;
use tracing::{error, info, warn};

mod message_handler;
mod setup_connection;

pub type SendTo = SendTo_<roles_logic_sv2::parsers::TemplateDistribution<'static>, ()>;
pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    /// Allows the tp recv to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
    jd: Option<Arc<Mutex<super::job_declarator::JobDeclarator>>>,
    down: Arc<Mutex<super::downstream::DownstreamMiningNode>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    new_template_message: Option<NewTemplate<'static>>,
    pool_chaneger_trigger: Arc<Mutex<PoolChangerTrigger>>,
    miner_coinbase_output: Vec<u8>,
    test_only_do_not_send_solution_to_tp: bool,
}

impl TemplateRx {
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
        test_only_do_not_send_solution_to_tp: bool,
    ) {
        let mut encoded_outputs = vec![];
        // jd is set to None in initialize_jd_as_solo_miner (in this case we need to take the first output as done by JDS)
        if jd.is_none() {
            miner_coinbase_outputs[0]
                .consensus_encode(&mut encoded_outputs)
                .expect("Invalid coinbase output in config");
        } else {
            miner_coinbase_outputs
                .consensus_encode(&mut encoded_outputs)
                .expect("Invalid coinbase output in config");
        }
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();

        let initiator = match authority_public_key {
            Some(pub_key) => Initiator::from_raw_k(pub_key.into_bytes()),
            None => Initiator::without_pk(),
        }
        .unwrap();
        let (mut receiver, mut sender, _, _) =
            Connection::new(stream, HandshakeRole::Initiator(initiator))
                .await
                .unwrap();

        info!("Template Receiver try to set up connection");
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
            test_only_do_not_send_solution_to_tp,
        }));

        let task = tokio::task::spawn(Self::on_new_solution(self_mutex.clone(), solution_receiver));
        task_collector
            .safe_lock(|c| c.push(task.abort_handle()))
            .unwrap();
        Self::start_templates(self_mutex);
    }

    pub async fn send(self_: &Arc<Mutex<Self>>, sv2_frame: StdFrame) {
        let either_frame = sv2_frame.into();
        let sender_to_tp = self_.safe_lock(|self_| self_.sender.clone()).unwrap();
        match sender_to_tp.send(either_frame).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
    }

    pub async fn send_max_coinbase_size(self_mutex: &Arc<Mutex<Self>>, size: u32) {
        let coinbase_output_data_size = PoolMessages::TemplateDistribution(
            TemplateDistribution::CoinbaseOutputDataSize(CoinbaseOutputDataSize {
                coinbase_output_max_additional_size: size,
            }),
        );
        let frame: StdFrame = coinbase_output_data_size.try_into().unwrap();
        Self::send(self_mutex, frame).await;
    }

    pub async fn send_tx_data_request(
        self_mutex: &Arc<Mutex<Self>>,
        new_template: NewTemplate<'static>,
    ) {
        let tx_data_request = PoolMessages::TemplateDistribution(
            TemplateDistribution::RequestTransactionData(RequestTransactionData {
                template_id: new_template.template_id,
            }),
        );
        let frame: StdFrame = tx_data_request.try_into().unwrap();
        Self::send(self_mutex, frame).await;
    }

    async fn get_last_token(
        jd: Option<Arc<Mutex<JobDeclarator>>>,
        miner_coinbase_output: &[u8],
    ) -> AllocateMiningJobTokenSuccess<'static> {
        if let Some(jd) = jd {
            super::job_declarator::JobDeclarator::get_last_token(&jd).await
        } else {
            AllocateMiningJobTokenSuccess {
                request_id: 0,
                mining_job_token: vec![0; 32].try_into().unwrap(),
                coinbase_output_max_additional_size: 100,
                coinbase_output: miner_coinbase_output.to_vec().try_into().unwrap(),
                async_mining_allowed: true,
            }
        }
    }

    pub fn start_templates(self_mutex: Arc<Mutex<Self>>) {
        let jd = self_mutex.safe_lock(|s| s.jd.clone()).unwrap();
        let down = self_mutex.safe_lock(|s| s.down.clone()).unwrap();
        let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
        let mut coinbase_output_max_additional_size_sent = false;
        let mut last_token = None;
        let miner_coinbase_output = self_mutex
            .safe_lock(|s| s.miner_coinbase_output.clone())
            .unwrap();
        let main_task = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                // Send CoinbaseOutputDataSize size to TP
                loop {
                    if last_token.is_none() {
                        let jd = self_mutex.safe_lock(|s| s.jd.clone()).unwrap();
                        last_token =
                            Some(Self::get_last_token(jd, &miner_coinbase_output[..]).await);
                    }
                    if !coinbase_output_max_additional_size_sent {
                        coinbase_output_max_additional_size_sent = true;
                        Self::send_max_coinbase_size(
                            &self_mutex,
                            last_token
                                .clone()
                                .unwrap()
                                .coinbase_output_max_additional_size,
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

                    let next_message_to_send =
                        ParseServerTemplateDistributionMessages::handle_message_template_distribution(
                            self_mutex.clone(),
                            message_type,
                            payload,
                        );
                    match next_message_to_send {
                        Ok(SendTo::None(m)) => {
                            match m {
                                // Send the new template along with the token to the JD so that JD can
                                // declare the mining job
                                Some(TemplateDistribution::NewTemplate(m)) => {
                                    // See coment on the definition of the global for memory
                                    // ordering
                                    super::IS_NEW_TEMPLATE_HANDLED
                                        .store(false, std::sync::atomic::Ordering::Release);
                                    Self::send_tx_data_request(&self_mutex, m.clone()).await;
                                    self_mutex
                                        .safe_lock(|t| t.new_template_message = Some(m.clone()))
                                        .unwrap();
                                    let token = last_token.clone().unwrap();
                                    let pool_output = token.coinbase_output.to_vec();
                                    super::downstream::DownstreamMiningNode::on_new_template(
                                        &down,
                                        m.clone(),
                                        &pool_output[..],
                                    )
                                    .await
                                    .unwrap();
                                }
                                Some(TemplateDistribution::SetNewPrevHash(m)) => {
                                    info!("Received SetNewPrevHash, waiting for IS_NEW_TEMPLATE_HANDLED");
                                    // See coment on the definition of the global for memory
                                    // ordering
                                    while !super::IS_NEW_TEMPLATE_HANDLED
                                        .load(std::sync::atomic::Ordering::Acquire)
                                    {
                                        tokio::task::yield_now().await;
                                    }
                                    info!("IS_NEW_TEMPLATE_HANDLED ok");
                                    if let Some(jd) = jd.as_ref() {
                                        super::job_declarator::JobDeclarator::on_set_new_prev_hash(
                                            jd.clone(),
                                            m.clone(),
                                        );
                                    }
                                    super::downstream::DownstreamMiningNode::on_set_new_prev_hash(
                                        &down, m,
                                    )
                                    .await
                                    .unwrap();
                                }

                                Some(TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                                    // safe to unwrap because this message is received after the new
                                    // template message
                                    let transactions_data = m.transaction_list;
                                    let excess_data = m.excess_data;
                                    let m = self_mutex
                                        .safe_lock(|t| t.new_template_message.clone())
                                        .unwrap()
                                        .unwrap();
                                    let token = last_token.unwrap();
                                    last_token = None;
                                    let mining_token = token.mining_job_token.to_vec();
                                    let pool_coinbase_out = token.coinbase_output.to_vec();
                                    if let Some(jd) = jd.as_ref() {
                                        super::job_declarator::JobDeclarator::on_new_template(
                                            jd,
                                            m.clone(),
                                            mining_token,
                                            transactions_data,
                                            excess_data,
                                            pool_coinbase_out,
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

    async fn on_new_solution(self_: Arc<Mutex<Self>>, rx: Receiver<SubmitSolution<'static>>) {
        while let Ok(solution) = rx.recv().await {
            if !self_
                .safe_lock(|s| s.test_only_do_not_send_solution_to_tp)
                .unwrap()
            {
                let sv2_frame: StdFrame = PoolMessages::TemplateDistribution(
                    TemplateDistribution::SubmitSolution(solution),
                )
                .try_into()
                .expect("Failed to convert solution to sv2 frame!");
                Self::send(&self_, sv2_frame).await
            }
        }
    }
}
