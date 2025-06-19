//! ## Job Declarator Module
//!
//! It contains logic and constructs to connect to the Job Declarator Server (JDS) and process
//! messaging logic related to job declaration.
//!
//! It handles the lifecycle of declaring mining jobs to the JDS, including
//! allocating mining job tokens, sending job declarations based on templates,
//! processing responses from the JDS, and pushing block solutions.
//!
//! The module tightly couples with the `template_receiver` for new templates and
//! previous hash updates, and with the `upstream_sv2` module for communication
//! with the main pool instance (to send `SetCustomMiningJob(s)`).

pub mod message_handler;
use async_channel::{Receiver, Sender};
use std::{collections::HashMap, convert::TryInto};
use stratum_common::{
    network_helpers_sv2::noise_connection::Connection,
    roles_logic_sv2::{
        bitcoin::{
            consensus,
            consensus::{serialize, Decodable},
            hashes::Hash,
            Transaction, TxOut,
        },
        codec_sv2::{
            binary_sv2::{Seq0255, Seq064K, B016M, B064K, U256},
            HandshakeRole, Initiator, StandardEitherFrame, StandardSv2Frame,
        },
        handlers::SendTo_,
        job_declaration_sv2::{AllocateMiningJobTokenSuccess, PushSolution},
        mining_sv2::SubmitSharesExtended,
        parsers::{AnyMessage, JobDeclaration},
        template_distribution_sv2::SetNewPrevHash,
        utils::Mutex,
    },
};
use tokio::task::AbortHandle;
use tracing::{debug, error, info};

use async_recursion::async_recursion;
use nohash_hasher::BuildNoHashHasher;
use std::{net::SocketAddr, sync::Arc};
use stratum_common::roles_logic_sv2::{
    handlers::job_declaration::ParseJobDeclarationMessagesFromUpstream,
    job_declaration_sv2::{AllocateMiningJobToken, DeclareMiningJob},
    template_distribution_sv2::NewTemplate,
    utils::Id,
};

pub type Message = AnyMessage<'static>;
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
pub type StdFrame = StandardSv2Frame<Message>;

mod setup_connection;
use setup_connection::SetupConnectionHandler;

use super::{config::JobDeclaratorClientConfig, error::Error, upstream_sv2::Upstream};

/// Struct describing LastDeclareJob fields.
#[derive(Debug, Clone)]
pub struct LastDeclareJob {
    // The `DeclareMiningJob` message that was sent to the JDS.
    declare_job: DeclareMiningJob<'static>,
    // The `NewTemplate` message from which this job was derived.
    template: NewTemplate<'static>,
    // The `SetNewPrevHash` message associated with this job, if it's not a future template.
    prev_hash: Option<SetNewPrevHash<'static>>,
    // The pool's coinbase output(s) for this job
    coinbase_pool_output: Vec<u8>,
    // The list of transactions (as raw bytes) included in this job's template.
    tx_list: Seq064K<'static, B016M<'static>>,
}

/// Struct describing the JDC internal state and components.
#[derive(Debug)]
pub struct JobDeclarator {
    // Receiver channel for messages from the JDS.
    receiver: Receiver<StandardEitherFrame<AnyMessage<'static>>>,
    // Sender channel for messages to the JDS.
    sender: Sender<StandardEitherFrame<AnyMessage<'static>>>,
    // A pool of pre-allocated `AllocateMiningJobTokenSuccess` tokens received from the JDS.
    allocated_tokens: Vec<AllocateMiningJobTokenSuccess<'static>>,
    // A simple ID generator for tracking request IDs.
    req_ids: Id,
    // An array to store information about the last two `DeclareMiningJob` messages sent.
    // This is used to correlate `DeclareMiningJobSuccess` responses.
    last_declare_mining_jobs_sent: [Option<(u32, LastDeclareJob)>; 2],
    // The last received `SetNewPrevHash` message.
    last_set_new_prev_hash: Option<SetNewPrevHash<'static>>,
    // A counter to track how many `SetNewPrevHash` messages have been received
    /// since the last time a future job was promoted. Used to determine if
    /// a future job is still relevant when its `SetNewPrevHash` arrives.
    set_new_prev_hash_counter: u8,
    // A map storing information about future jobs (jobs derived from future templates)
    // received from the Template Provider, keyed by their template ID.
    // This information is kept until the corresponding `SetNewPrevHash` arrives,
    // at which point the future job can be promoted and declared to the upstream pool.
    #[allow(clippy::type_complexity)]
    future_jobs: HashMap<
        u64,
        (
            DeclareMiningJob<'static>,
            Seq0255<'static, U256<'static>>,
            NewTemplate<'static>,
            // pool's outputs
            Vec<u8>,
        ),
        BuildNoHashHasher<u64>,
    >,
    // `Upstream` instance, used for communicating with the main pool instance
    up: Arc<Mutex<Upstream>>,
    task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    // The prefix of the coinbase transaction,
    pub coinbase_tx_prefix: B064K<'static>,
    // The suffix of the coinbase transaction,
    pub coinbase_tx_suffix: B064K<'static>,
}

impl JobDeclarator {
    /// Instantiates a new `JobDeclarator` client, connects to the provided JDS address,
    /// performs the SV2 setup connection handshake, allocates initial mining job tokens,
    /// and starts the background task for processing messages from the JDS.
    pub async fn new(
        address: SocketAddr,
        authority_public_key: [u8; 32],
        config: JobDeclaratorClientConfig,
        up: Arc<Mutex<Upstream>>,
        task_collector: Arc<Mutex<Vec<AbortHandle>>>,
    ) -> Result<Arc<Mutex<Self>>, Error<'static>> {
        let stream = tokio::net::TcpStream::connect(address).await?;
        let initiator = Initiator::from_raw_k(authority_public_key)?;
        let (mut receiver, mut sender) =
            Connection::new(stream, HandshakeRole::Initiator(initiator))
                .await
                .expect("impossible to connect");

        info!(
            "JD Client: SETUP_CONNECTION address: {:?}",
            config.listening_address()
        );

        SetupConnectionHandler::setup(&mut receiver, &mut sender, *config.listening_address())
            .await
            .unwrap();

        info!("JD CONNECTED");

        let self_ = Arc::new(Mutex::new(JobDeclarator {
            receiver,
            sender,
            allocated_tokens: vec![],
            req_ids: Id::new(),
            last_declare_mining_jobs_sent: [None, None],
            last_set_new_prev_hash: None,
            future_jobs: HashMap::with_hasher(BuildNoHashHasher::default()),
            up,
            task_collector,
            coinbase_tx_prefix: vec![].try_into().unwrap(),
            coinbase_tx_suffix: vec![].try_into().unwrap(),
            set_new_prev_hash_counter: 0,
        }));

        Self::allocate_tokens(&self_, 2).await;
        Self::on_upstream_message(self_.clone());
        Ok(self_)
    }

    // Utility method to retrieve information about a previously sent `DeclareMiningJob`
    // from the `last_declare_mining_jobs_sent` window based on its request ID.
    fn get_last_declare_job_sent(
        self_mutex: &Arc<Mutex<Self>>,
        request_id: u32,
    ) -> Option<LastDeclareJob> {
        self_mutex
            .safe_lock(|s| {
                for (id, job) in s.last_declare_mining_jobs_sent.iter().flatten() {
                    if *id == request_id {
                        return Some(job.to_owned());
                    }
                }
                None
            })
            .unwrap()
    }

    // We maintain a window of 2 jobs. If more than 2 blocks are found,
    // the ordering will depend on the request ID. Only the 2 most recent request
    // IDs will be kept in memory, while the rest will be discarded.
    // More information can be found here: https://github.com/stratum-mining/stratum/pull/904#discussion_r1609469048
    fn update_last_declare_job_sent(
        self_mutex: &Arc<Mutex<Self>>,
        request_id: u32,
        j: LastDeclareJob,
    ) {
        self_mutex
            .safe_lock(|s| {
                if let Some(empty_index) = s
                    .last_declare_mining_jobs_sent
                    .iter()
                    .position(|entry| entry.is_none())
                {
                    s.last_declare_mining_jobs_sent[empty_index] = Some((request_id, j));
                } else if let Some((min_index, _)) = s
                    .last_declare_mining_jobs_sent
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| entry.as_ref().map(|(id, _)| (i, id)))
                    .min_by_key(|&(_, id)| id)
                {
                    s.last_declare_mining_jobs_sent[min_index] = Some((request_id, j));
                }
            })
            .unwrap();
    }

    /// This method retrieves an allocated mining job token from the internal pool.
    ///
    /// If the pool of allocated tokens is empty or low, this method triggers the
    /// allocation of more tokens from the JDS and waits until tokens are available
    /// before returning one. This ensures that tokens are always available when
    /// needed to declare new jobs.
    #[async_recursion]
    pub async fn get_last_token(
        self_mutex: &Arc<Mutex<Self>>,
    ) -> AllocateMiningJobTokenSuccess<'static> {
        // Check the current number of allocated tokens.
        let mut token_len = self_mutex.safe_lock(|s| s.allocated_tokens.len()).unwrap();
        match token_len {
            0 => {
                // If no tokens are available, spawn a task to allocate more (2 tokens).
                {
                    let task = {
                        let self_mutex = self_mutex.clone();
                        tokio::task::spawn(async move {
                            Self::allocate_tokens(&self_mutex, 2).await;
                        })
                    };
                    // Add the allocation task's handle to the collector.
                    self_mutex
                        .safe_lock(|s| {
                            s.task_collector
                                .safe_lock(|c| c.push(task.abort_handle()))
                                .unwrap()
                        })
                        .unwrap();
                }

                // Wait until at least one token becomes available to avoid infinite recursion.
                while token_len == 0 {
                    tokio::task::yield_now().await;
                    token_len = self_mutex.safe_lock(|s| s.allocated_tokens.len()).unwrap();
                }
                // Once tokens are available, recursively call get_last_token to retrieve one.
                Self::get_last_token(self_mutex).await
            }
            1 => {
                // If only one token is available, spawn a task to allocate one more
                // to maintain a buffer, but return the current token immediately.
                {
                    let task = {
                        let self_mutex = self_mutex.clone();
                        tokio::task::spawn(async move {
                            Self::allocate_tokens(&self_mutex, 1).await;
                        })
                    };
                    // Add the allocation task's handle to the collector.
                    self_mutex
                        .safe_lock(|s| {
                            s.task_collector
                                .safe_lock(|c| c.push(task.abort_handle()))
                                .unwrap()
                        })
                        .unwrap();
                }
                // There is a token, unwrap is safe
                self_mutex
                    .safe_lock(|s| s.allocated_tokens.pop())
                    .unwrap()
                    .unwrap()
            }
            // There are tokens, unwrap is safe
            _ => self_mutex
                .safe_lock(|s| s.allocated_tokens.pop())
                .unwrap()
                .unwrap(),
        }
    }

    /// Handles the event of a new template being received from the Template Receiver.
    ///
    /// This method constructs a `DeclareMiningJob` message based on the new template,
    /// an allocated mining job token, and the miner's coinbase transaction parts.
    /// It then updates the window of last sent jobs and sends the `DeclareMiningJob`
    /// message to the JDS.
    pub async fn on_new_template(
        self_mutex: &Arc<Mutex<Self>>,
        template: NewTemplate<'static>,
        token: Vec<u8>,
        tx_list_: Seq064K<'static, B016M<'static>>,
        excess_data: B064K<'static>,
        coinbase_pool_output: Vec<u8>,
    ) {
        // Get a new request ID and clone the sender for sending the message.
        let (id, sender) = self_mutex
            .safe_lock(|s| (s.req_ids.next(), s.sender.clone()))
            .unwrap();

        // Deserialize the transaction list to calculate short hashes and the list hash.
        let mut tx_list: Vec<Transaction> = Vec::new();
        let mut txids_as_u256: Vec<U256<'static>> = Vec::new();
        for tx in tx_list_.to_vec() {
            //TODO remove unwrap
            let tx: Transaction = consensus::deserialize(&tx).unwrap();
            let txid = tx.compute_txid();
            let byte_array: [u8; 32] = *txid.as_byte_array();
            let owned_vec: Vec<u8> = byte_array.into();
            let txid_as_u256 = U256::Owned(owned_vec);
            txids_as_u256.push(txid_as_u256);
            tx_list.push(tx);
        }
        let tx_ids = Seq064K::new(txids_as_u256).expect("Failed to create Seq064K");

        // Construct the DeclareMiningJob message.
        let declare_job = DeclareMiningJob {
            request_id: id,
            mining_job_token: token.try_into().unwrap(),
            version: template.version,
            coinbase_prefix: self_mutex
                .safe_lock(|s| s.coinbase_tx_prefix.clone())
                .unwrap(),
            coinbase_suffix: self_mutex
                .safe_lock(|s| s.coinbase_tx_suffix.clone())
                .unwrap(),
            tx_ids_list: tx_ids,
            excess_data, // request transaction data
        };

        // Determine the associated SetNewPrevHash message. This is only relevant
        // if the template is *not* a future template.
        let prev_hash = self_mutex
            .safe_lock(|s| s.last_set_new_prev_hash.clone())
            .unwrap()
            .filter(|_| !template.future_template);

        // Store information about this declared job in the window for tracking responses.
        let last_declare = LastDeclareJob {
            declare_job: declare_job.clone(),
            template,
            prev_hash,
            coinbase_pool_output,
            tx_list: tx_list_.clone(),
        };
        Self::update_last_declare_job_sent(self_mutex, id, last_declare);
        let frame: StdFrame =
            AnyMessage::JobDeclaration(JobDeclaration::DeclareMiningJob(declare_job))
                .try_into()
                .unwrap();
        sender.send(frame.into()).await.unwrap();
    }

    /// This method contains the core logic for processing incoming messages from the JDS.
    ///
    /// It runs in a background task and continuously receives messages from the JDS.
    /// It dispatches these messages to the appropriate handlers defined in the
    /// `ParseJobDeclarationMessagesFromUpstream` trait implementation.
    /// Based on the handler's response, it may process the message further
    /// (e.g., handling `DeclareMiningJobSuccess` or `DeclareMiningJobError`) or
    /// send a response back to the JDS if required.
    pub fn on_upstream_message(self_mutex: Arc<Mutex<Self>>) {
        let up = self_mutex.safe_lock(|s| s.up.clone()).unwrap();
        // Spawn the main task for receiving and processing JDS messages.
        let main_task = {
            let self_mutex = self_mutex.clone();
            tokio::task::spawn(async move {
                let receiver = self_mutex.safe_lock(|d| d.receiver.clone()).unwrap();
                loop {
                    let mut incoming: StdFrame = receiver.recv().await.unwrap().try_into().unwrap();
                    let message_type = incoming.get_header().unwrap().msg_type();
                    let payload = incoming.payload();
                    let next_message_to_send =
                        ParseJobDeclarationMessagesFromUpstream::handle_message_job_declaration(
                            self_mutex.clone(),
                            message_type,
                            payload,
                        );
                    // Process the result of the message handling.
                    match next_message_to_send {
                        // Handle a successful job declaration response.
                        Ok(SendTo::None(Some(JobDeclaration::DeclareMiningJobSuccess(m)))) => {
                            let new_token = m.new_mining_job_token;
                            // Retrieve the information about the original DeclareMiningJob using
                            // the request ID.
                            let last_declare = Self::get_last_declare_job_sent(&self_mutex, m.request_id).unwrap_or_else(|| panic!("Failed to get last declare job: job not found, Request Id: {:?}.", m.request_id));
                            debug!("LastDeclareJob.prev_hash: {:?}", last_declare.prev_hash);
                            let mut last_declare_mining_job_sent = last_declare.declare_job;
                            let is_future = last_declare.template.future_template;
                            let id = last_declare.template.template_id;
                            let merkle_path = last_declare.template.merkle_path.clone();
                            let template = last_declare.template;

                            // TODO: Signaling mechanism needed here to inform on_set_new_prev_hash
                            // that the token has been updated, so it can decide whether to send
                            // SetCustomJobs.
                            if is_future {
                                // If it was a future job, update its mining job token and store it
                                // in the future_jobs map.
                                last_declare_mining_job_sent.mining_job_token = new_token;
                                self_mutex
                                    .safe_lock(|s| {
                                        s.future_jobs.insert(
                                            id,
                                            (
                                                last_declare_mining_job_sent,
                                                merkle_path,
                                                template,
                                                last_declare.coinbase_pool_output,
                                            ),
                                        );
                                    })
                                    .unwrap();
                            } else {
                                // If it was a non-future job, it should have an associated
                                // SetNewPrevHash.
                                let set_new_prev_hash = last_declare.prev_hash;

                                let mut template_coinbase_outputs = Vec::<TxOut>::consensus_decode(
                                    &mut template
                                        .coinbase_tx_outputs
                                        .inner_as_ref()
                                        .to_vec()
                                        .as_slice(),
                                )
                                .expect("Failed to deserialize template outputs");

                                // temporary workaround for https://github.com/Sjors/bitcoin/issues/92
                                if template_coinbase_outputs.is_empty() {
                                    template_coinbase_outputs = vec![TxOut::consensus_decode(
                                        &mut template
                                            .coinbase_tx_outputs
                                            .inner_as_ref()
                                            .to_vec()
                                            .as_slice(),
                                    )
                                    .expect("Failed to deserialize template outputs")];
                                }

                                let mut pool_coinbase_outputs = Vec::<TxOut>::consensus_decode(
                                    &mut last_declare.coinbase_pool_output.as_slice(),
                                )
                                .expect("Failed to deserialize pool outputs");
                                pool_coinbase_outputs.append(&mut template_coinbase_outputs);
                                let serialized_pool_outs = serialize(&pool_coinbase_outputs);
                                match set_new_prev_hash {
                                    // Send the SetCustomJobs message to the upstream pool.
                                    Some(p) => Upstream::set_custom_jobs(
                                        &up,
                                        last_declare_mining_job_sent,
                                        p,
                                        merkle_path,
                                        new_token,
                                        template.coinbase_tx_version,
                                        template.coinbase_prefix,
                                        template.coinbase_tx_input_sequence,
                                        serialized_pool_outs,
                                        template.coinbase_tx_locktime,
                                        template.template_id
                                        ).await.unwrap(),
                                    None => panic!("Invalid state we received a NewTemplate not future, without having received a set new prev hash")
                                }
                            }
                        }
                        Ok(SendTo::None(Some(JobDeclaration::DeclareMiningJobError(m)))) => {
                            error!("Job is not verified: {}", m);
                        }
                        Ok(SendTo::None(None)) => (),
                        Ok(SendTo::Respond(m)) => {
                            let sv2_frame: StdFrame =
                                AnyMessage::JobDeclaration(m).try_into().unwrap();
                            let sender =
                                self_mutex.safe_lock(|self_| self_.sender.clone()).unwrap();
                            sender.send(sv2_frame.into()).await.unwrap();
                        }
                        Ok(_) => unreachable!(),
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

    /// Handles the event of a `SetNewPrevHash` being received
    /// from the Template Receiver.
    ///
    /// This method updates the stored last `SetNewPrevHash` if the new one is
    /// more recent. It then checks if a corresponding future job is stored.
    /// If a matching future job is found and is still relevant (based on the
    /// `set_new_prev_hash_counter`), the future job is promoted, removed from
    /// the future jobs map, and a `SetCustomJobs` message is sent to the upstream
    /// pool to activate this job for mining.
    pub fn on_set_new_prev_hash(
        self_mutex: Arc<Mutex<Self>>,
        set_new_prev_hash: SetNewPrevHash<'static>,
    ) {
        // Spawn a task to handle the SetNewPrevHash
        tokio::task::spawn(async move {
            let id = set_new_prev_hash.template_id;
            // Update the last_set_new_prev_hash if the new one is more recent.
            let _ = self_mutex.safe_lock(|s| {
                debug!("Before update - last_set_new_prev_hash: {:?}, set_new_prev_hash_counter: {}",
                s.last_set_new_prev_hash, s.set_new_prev_hash_counter);
                let should_update = s
                    .last_set_new_prev_hash
                    .as_ref()
                    .map(|prev| set_new_prev_hash.template_id > prev.template_id)
                    .unwrap_or(true);

                if should_update {
                    s.last_set_new_prev_hash = Some(set_new_prev_hash.clone());
                    s.set_new_prev_hash_counter += 1;
                    debug!("After update - last_set_new_prev_hash updated to: {:?}, set_new_prev_hash_counter: {}",
                    s.last_set_new_prev_hash, s.set_new_prev_hash_counter);
                } else {
                    debug!("Received outdated SetNewPrevHash: {:?} compared to current: {:?}",
                    set_new_prev_hash, s.last_set_new_prev_hash);
                }
            });
            // Loop to find and promote the corresponding future job.
            let (job, up, merkle_path, template, pool_outs) = loop {
                match self_mutex
                    .safe_lock(|s| {
                        // Check if the received SetNewPrevHash is outdated based on the counter
                        if s.set_new_prev_hash_counter > 1
                            && s.last_set_new_prev_hash != Some(set_new_prev_hash.clone())
                        {
                            debug!(
                                "Declared job {} skipped due to set_new_prev_hash_counter",
                                id
                            );
                            s.set_new_prev_hash_counter -= 1;
                            Some(None)
                        } else {
                            // Attempt to remove and retrieve the future job matching the template
                            // ID.
                            s.future_jobs.remove(&id).map(
                                |(job, merkle_path, template, pool_outs)| {
                                    s.future_jobs =
                                        HashMap::with_hasher(BuildNoHashHasher::default());
                                    s.set_new_prev_hash_counter -= 1;
                                    Some((job, s.up.clone(), merkle_path, template, pool_outs))
                                },
                            )
                        }
                    })
                    .unwrap()
                {
                    Some(Some(future_job_tuple)) => break future_job_tuple,
                    Some(None) => return,
                    None => {}
                };
                tokio::task::yield_now().await;
            };
            // The token received from JDS for this job.
            let signed_token = job.mining_job_token.clone();
            // Prepare the pool's coinbase output by appending the template's outputs.
            let mut template_coinbase_outputs = Vec::<TxOut>::consensus_decode(
                &mut template
                    .coinbase_tx_outputs
                    .inner_as_ref()
                    .to_vec()
                    .as_slice(),
            )
            .expect("Failed to deserialize template outputs");

            // temporary workaround for https://github.com/Sjors/bitcoin/issues/92
            if template_coinbase_outputs.is_empty() {
                template_coinbase_outputs = vec![TxOut::consensus_decode(
                    &mut template
                        .coinbase_tx_outputs
                        .inner_as_ref()
                        .to_vec()
                        .as_slice(),
                )
                .expect("Failed to deserialize template outputs")];
            }

            let mut pool_coinbase_outputs =
                Vec::<TxOut>::consensus_decode(&mut pool_outs.as_slice())
                    .expect("Failed to deserialize pool outputs");
            pool_coinbase_outputs.append(&mut template_coinbase_outputs);

            let serialized_pool_outs = serialize(&pool_coinbase_outputs);

            // Send the SetCustomJobs message to the upstream pool to activate this job.
            Upstream::set_custom_jobs(
                &up,
                job,
                set_new_prev_hash,
                merkle_path,
                signed_token,
                template.coinbase_tx_version,
                template.coinbase_prefix,
                template.coinbase_tx_input_sequence,
                serialized_pool_outs,
                template.coinbase_tx_locktime,
                template.template_id,
            )
            .await
            .unwrap();
        });
    }

    /// Sends `AllocateMiningJobToken` messages to the JDS to request new mining job tokens.
    ///
    /// This method is typically called when the internal pool of allocated tokens
    /// is low, ensuring that tokens are available for future job declarations.
    async fn allocate_tokens(self_mutex: &Arc<Mutex<Self>>, token_to_allocate: u32) {
        for i in 0..token_to_allocate {
            let message = JobDeclaration::AllocateMiningJobToken(AllocateMiningJobToken {
                user_identifier: "todo".to_string().try_into().unwrap(),
                request_id: i,
            });
            let sender = self_mutex.safe_lock(|s| s.sender.clone()).unwrap();
            // Safe unwrap message is build above and is valid, below can never panic
            let frame: StdFrame = AnyMessage::JobDeclaration(message).try_into().unwrap();
            // TODO join re
            sender.send(frame.into()).await.unwrap();
        }
    }

    /// Handles the event of a miner solution being received from the downstream.
    ///
    /// This method constructs a `PushSolution` message containing the necessary
    /// solution details and sends it to the JDS.
    pub async fn on_solution(
        self_mutex: &Arc<Mutex<Self>>,
        solution: SubmitSharesExtended<'static>,
    ) {
        // Retrieve the last received SetNewPrevHash message.
        let prev_hash = self_mutex
            .safe_lock(|s| s.last_set_new_prev_hash.clone())
            .unwrap()
            .expect("");

        // Construct the PushSolution message using details from the solution and the last prev
        // hash
        let solution = PushSolution {
            extranonce: solution.extranonce,
            prev_hash: prev_hash.prev_hash,
            ntime: solution.ntime,
            nonce: solution.nonce,
            nbits: prev_hash.n_bits,
            version: solution.version,
        };
        let frame: StdFrame = AnyMessage::JobDeclaration(JobDeclaration::PushSolution(solution))
            .try_into()
            .unwrap();
        let sender = self_mutex.safe_lock(|s| s.sender.clone()).unwrap();
        sender.send(frame.into()).await.unwrap();
    }
}
