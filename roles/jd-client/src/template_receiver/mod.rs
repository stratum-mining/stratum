use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::{
    template_distribution_sv2::{NewTemplate, RequestTransactionData},
    utils::Mutex,
};

use codec_sv2::Frame;
use roles_logic_sv2::{
    handlers::{template_distribution::ParseServerTemplateDistributionMessages, SendTo_},
    parsers::{PoolMessages, TemplateDistribution},
    template_distribution_sv2::{CoinbaseOutputDataSize, SubmitSolution},
};
pub type SendTo = SendTo_<roles_logic_sv2::parsers::TemplateDistribution<'static>, ()>;
//use messages_sv2::parsers::JobDeclaration;
pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
use async_channel::{Receiver, Sender};
use network_helpers::plain_connection_tokio::PlainConnection;
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tokio::task::AbortHandle;
mod message_handler;
mod setup_connection;
use crate::status;
use error_handling::handle_result;
use setup_connection::SetupConnectionHandler;
use tracing::info;

pub struct TemplateRx {
    receiver: Receiver<EitherFrame>,
    sender: Sender<EitherFrame>,
    /// Allows the tp recv to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
    jd: Arc<Mutex<crate::job_declarator::JobDeclarator>>,
    down: Arc<Mutex<crate::downstream::DownstreamMiningNode>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    new_template_message: Option<NewTemplate<'static>>,
}

impl TemplateRx {
    pub async fn connect(
        address: SocketAddr,
        solution_receiver: Receiver<SubmitSolution<'static>>,
        tx_status: status::Sender,
        jd: Arc<Mutex<crate::job_declarator::JobDeclarator>>,
        down: Arc<Mutex<crate::downstream::DownstreamMiningNode>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    ) {
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();

        let (mut receiver, mut sender): (Receiver<EitherFrame>, Sender<EitherFrame>) =
            PlainConnection::new(stream).await;

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
        info!("sending RequestTransactionData...");
        Self::send(self_mutex, frame).await;
    }

    pub fn start_templates(self_mutex: Arc<Mutex<Self>>) {
        let jd = self_mutex.safe_lock(|s| s.jd.clone()).unwrap();
        let down = self_mutex.safe_lock(|s| s.down.clone()).unwrap();
        let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
        let mut coinbase_output_max_additional_size_sent = false;
        let mut last_token = None;
        let main_task = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                // Send CoinbaseOutputDataSize size to TP
                loop {
                    if last_token.is_none() {
                        last_token =
                            Some(crate::job_declarator::JobDeclarator::get_last_token(&jd).await);
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
                                    crate::IS_NEW_TEMPLATE_HANDLED
                                        .store(false, std::sync::atomic::Ordering::Relaxed);
                                    Self::send_tx_data_request(&self_mutex, m.clone()).await;
                                    self_mutex
                                        .safe_lock(|t| t.new_template_message = Some(m.clone()))
                                        .unwrap();
                                    info!(
                                        "NEW_TEMPLATE: {:?}",
                                        self_mutex
                                            .safe_lock(|t| t.new_template_message.clone())
                                            .unwrap()
                                            .unwrap()
                                    );
                                    let token = last_token.clone().unwrap();
                                    let pool_output = token.coinbase_output.to_vec();
                                    let res_down =
                                        crate::downstream::DownstreamMiningNode::on_new_template(
                                            &down,
                                            m,
                                            &pool_output[..],
                                        )
                                        .await;
                                    res_down.unwrap();
                                }
                                Some(TemplateDistribution::SetNewPrevHash(m)) => {
                                    info!("Received SetNewPrevHash, waiting for IS_NEW_TEMPLATE_HANDLED");
                                    // See coment on the definition of the global for memory
                                    // ordering
                                    while !crate::IS_NEW_TEMPLATE_HANDLED
                                        .load(std::sync::atomic::Ordering::Acquire)
                                    {
                                        tokio::task::yield_now().await;
                                    }
                                    info!("IS_NEW_TEMPLATE_HANDLED ok");
                                    let (res_down, _) = tokio::join!(
                                    crate::downstream::DownstreamMiningNode::on_set_new_prev_hash(
                                        &down,
                                        m.clone()
                                    ),
                                    crate::job_declarator::JobDeclarator::on_set_new_prev_hash(&jd, m),
                                );
                                    res_down.unwrap();
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
                                    let pool_output = token.coinbase_output.to_vec();
                                    crate::job_declarator::JobDeclarator::on_new_template(
                                        &jd,
                                        m.clone(),
                                        mining_token,
                                        pool_output.clone(),
                                        transactions_data,
                                        excess_data,
                                    )
                                    .await;
                                }
                                _ => todo!(),
                            }
                        }
                        Ok(_) => panic!(),
                        Err(_) => todo!(),
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
            let sv2_frame: StdFrame =
                PoolMessages::TemplateDistribution(TemplateDistribution::SubmitSolution(solution))
                    .try_into()
                    .expect("Failed to convert solution to sv2 frame!");
            Self::send(&self_, sv2_frame).await
        }
    }
}
