//! # Bitcoin Core Sv2 Library
//!
//! A library to interact with Bitcoin Core via [Sv2 Template Distribution Protocol](https://github.com/stratum-mining/sv2-spec/blob/main/07-Template-Distribution-Protocol.md).
//!
//! It leverages [`bitcoin_capnp`] to interact with Bitcoin Core via IPC over a UNIX socket.

pub mod error;

use crate::template_data::TemplateData;
use async_channel::{Receiver, Sender};
use binary_sv2::U256;
use bitcoin_capnp::{
    init_capnp::init::Client as InitIpcClient,
    mining_capnp::{
        block_template::Client as BlockTemplateIpcClient, mining::Client as MiningIpcClient,
    },
    proxy_capnp::{thread::Client as ThreadIpcClient, thread_map::Client as ThreadMapIpcClient},
};
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use error::BitcoinCoreSv2Error;
use parsers_sv2::TemplateDistribution;
use roles_logic_sv2::bitcoin::{block::Block, consensus::deserialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::Path,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use template_distribution_sv2::{
    CoinbaseOutputConstraints, RequestTransactionData, RequestTransactionDataError, SubmitSolution,
};
use tokio::{net::UnixStream, sync::RwLock};
use tokio_util::{compat::*, sync::CancellationToken};
use tracing::info;

mod template_data;

/// The main abstraction for interacting with Bitcoin Core via Sv2 Template Distribution Protocol.
///
/// It is instantiated with:
/// - A `&`[`std::path::Path`] to the Bitcoin Core UNIX socket
/// - A [`template_distribution_sv2::CoinbaseOutputConstraints`] message
/// - A `u64` for the fee delta threshold in sats
/// - A [`async_channel::Receiver`] for incoming
///   [`template_distribution_sv2::RequestTransactionData`] messages
/// - A [`async_channel::Receiver`] for incoming [`template_distribution_sv2::SubmitSolution`]
///   messages
/// - A [`async_channel::Sender`] for outgoing [`parsers_sv2::TemplateDistribution`] messages
/// - A [`tokio_util::sync::CancellationToken`] to stop the internally spawned tasks
///
/// Upon creation, the [`BitcoinCoreSv2`] instance sends a
/// [`template_distribution_sv2::NewTemplate`] followed by a corresponding
/// [`template_distribution_sv2::SetNewPrevHash`] message over the outgoing channel.
///
/// As configured via `fee_threshold`, the [`BitcoinCoreSv2`] instance will monitor the
/// mempool for changes and send a [`template_distribution_sv2::NewTemplate`] message if the fee
/// delta is greater than the configured threshold.
///
/// When there's a new Chain Tip, the [`BitcoinCoreSv2`] instance will send a
/// [`template_distribution_sv2::NewTemplate`] followed by a corresponding
/// [`template_distribution_sv2::SetNewPrevHash`] message over the outgoing channel.
///
/// Incoming [`template_distribution_sv2::RequestTransactionData`] messages are used to request
/// transactions relative to a specific template, for which a corresponding
/// [`template_distribution_sv2::RequestTransactionDataSuccess`] or
/// [`template_distribution_sv2::RequestTransactionDataError`] message is sent over the outgoing
/// channel.
///
/// Incoming [`template_distribution_sv2::SubmitSolution`] messages are used to submit solutions to
/// a specific template.
#[derive(Clone)]
pub struct BitcoinCoreSv2 {
    fee_threshold: u64,
    thread_ipc_client: ThreadIpcClient,
    current_template_ipc_client: Rc<RefCell<BlockTemplateIpcClient>>,
    template_data: Rc<RwLock<HashMap<u64, TemplateData>>>,
    stale_template_ids: Rc<RwLock<HashSet<u64>>>,
    template_id_factory: Rc<AtomicU64>,
    request_transaction_data_receiver: Receiver<RequestTransactionData>,
    submit_solution_receiver: Receiver<SubmitSolution<'static>>,
    outgoing_template_distribution_message_sender: Sender<TemplateDistribution<'static>>,
    cancellation_token: CancellationToken,
}

impl BitcoinCoreSv2 {
    /// Creates a new [`BitcoinCoreSv2`] instance.
    pub async fn new(
        bitcoin_core_unix_socket_path: &Path,
        coinbase_output_constraints: CoinbaseOutputConstraints,
        fee_threshold: u64,
        request_transaction_data_receiver: Receiver<RequestTransactionData>,
        submit_solution_receiver: Receiver<SubmitSolution<'static>>,
        outgoing_messages: Sender<TemplateDistribution<'static>>,
        cancellation_token: CancellationToken,
    ) -> Result<Self, BitcoinCoreSv2Error> {
        info!(
            "Creating new Sv2 Bitcoin Core Connection via IPC over UNIX socket: {}",
            bitcoin_core_unix_socket_path.display()
        );
        let coinbase_output_max_additional_size =
            coinbase_output_constraints.coinbase_output_max_additional_size;
        let coinbase_output_max_additional_sigops =
            coinbase_output_constraints.coinbase_output_max_additional_sigops;
        info!(
            "Coinbase constraints: max additional size: {}, max additional sigops: {}",
            coinbase_output_max_additional_size, coinbase_output_max_additional_sigops
        );

        let stream = UnixStream::connect(bitcoin_core_unix_socket_path)
            .await
            .map_err(|_| {
                BitcoinCoreSv2Error::CannotConnectToUnixSocket(bitcoin_core_unix_socket_path.into())
            })?;
        let (reader, writer) = stream.into_split();
        let reader_compat = reader.compat();
        let writer_compat = writer.compat_write();

        let rpc_network = Box::new(twoparty::VatNetwork::new(
            reader_compat,
            writer_compat,
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        ));

        let mut rpc_system = RpcSystem::new(rpc_network, None);
        let bootstrap_client: InitIpcClient =
            rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

        tokio::task::spawn_local(rpc_system);

        let construct_response = bootstrap_client.construct_request().send().promise.await?;

        let thread_map: ThreadMapIpcClient = construct_response.get()?.get_thread_map()?;
        let thread_request = thread_map.make_thread_request();
        let thread_response = thread_request.send().promise.await?;

        let thread_ipc_client: ThreadIpcClient = thread_response.get()?.get_result()?;

        info!("IPC execution thread client successfully created.");

        let mut mining_client_request = bootstrap_client.make_mining_request();
        mining_client_request
            .get()
            .get_context()?
            .set_thread(thread_ipc_client.clone());
        let mining_client_response = mining_client_request.send().promise.await?;
        let mining_ipc_client: MiningIpcClient = mining_client_response.get()?.get_result()?;

        info!("IPC mining client successfully created.");

        let mut template_ipc_client_request = mining_ipc_client.create_new_block_request();
        let mut template_ipc_client_request_options =
            template_ipc_client_request.get().get_options()?;

        let coinbase_weight = (coinbase_output_max_additional_size * 4) as u64;
        let block_reserved_weight = coinbase_weight.max(2000); // 2000 is the minimum block reserved weight
        template_ipc_client_request_options.set_block_reserved_weight(block_reserved_weight);
        template_ipc_client_request_options.set_coinbase_output_max_additional_sigops(
            coinbase_output_max_additional_sigops as u64,
        );
        template_ipc_client_request_options.set_use_mempool(true);

        let template_ipc_client = template_ipc_client_request
            .send()
            .promise
            .await?
            .get()?
            .get_result()?;

        Ok(Self {
            fee_threshold,
            thread_ipc_client,
            template_id_factory: Rc::new(AtomicU64::new(0)),
            current_template_ipc_client: Rc::new(RefCell::new(template_ipc_client)),
            template_data: Rc::new(RwLock::new(HashMap::new())),
            stale_template_ids: Rc::new(RwLock::new(HashSet::new())),
            cancellation_token,
            request_transaction_data_receiver,
            submit_solution_receiver,
            outgoing_template_distribution_message_sender: outgoing_messages,
        })
    }

    /// Runs the [`BitcoinCoreSv2`] instance, monitoring for:
    /// - Chain Tip changes, for which it will send a [`template_distribution_sv2::NewTemplate`]
    ///   message, followed by a [`template_distribution_sv2::SetNewPrevHash`] message
    /// - incoming [`template_distribution_sv2::RequestTransactionData`] messages, for which it will
    ///   send a [`template_distribution_sv2::RequestTransactionDataSuccess`] or
    ///   [`template_distribution_sv2::RequestTransactionDataError`] message as a response
    /// - incoming [`template_distribution_sv2::SubmitSolution`] messages, for which it will submit
    ///   the solution to the Bitcoin Core IPC client
    ///
    /// Blocks until the cancellation token is activated.
    pub async fn run(&self) {
        self.monitor_ipc_templates();
        self.monitor_request_transaction_data();
        self.monitor_submit_solution();

        // block until the cancellation token is activated
        self.cancellation_token.cancelled().await;
    }

    async fn bootstrap_first_template(&self) -> Result<U256, BitcoinCoreSv2Error> {
        let template_data = match self.fetch_template_data().await {
            Ok(template_data) => template_data,
            Err(e) => {
                tracing::error!("Failed to fetch template data: {:?}", e);
                return Err(e);
            }
        };

        // send the future NewTemplate message
        let future_template = template_data.get_new_template_message(true);

        match self
            .outgoing_template_distribution_message_sender
            .send(TemplateDistribution::NewTemplate(future_template.clone()))
            .await
        {
            Ok(_) => (),
            Err(e) => {
                tracing::error!("Failed to send future template message: {:?}", e);
                return Err(BitcoinCoreSv2Error::FailedToSendNewTemplateMessage);
            }
        }

        // send the SetNewPrevHash message
        let set_new_prev_hash = template_data.get_set_new_prev_hash_message();

        match self
            .outgoing_template_distribution_message_sender
            .send(TemplateDistribution::SetNewPrevHash(
                set_new_prev_hash.clone(),
            ))
            .await
        {
            Ok(_) => (),
            Err(e) => {
                tracing::error!("Failed to send set new prev hash message: {:?}", e);
                return Err(BitcoinCoreSv2Error::FailedToSendSetNewPrevHashMessage);
            }
        }

        // save the template data
        self.template_data
            .write()
            .await
            .insert(template_data.get_template_id(), template_data.clone());

        Ok(template_data.get_prev_hash())
    }

    fn monitor_ipc_templates(&self) {
        let self_clone = self.clone();

        tokio::task::spawn_local(async move {
            // bootstrap the first template
            let mut prev_hash = match self_clone.bootstrap_first_template().await {
                Ok(prev_hash) => prev_hash,
                Err(e) => {
                    tracing::error!("Failed to bootstrap first template: {:?}", e);
                    tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                    self_clone.cancellation_token.cancel();
                    return;
                }
            };

            loop {
                // Create a new request for each iteration
                let mut wait_next_request = self_clone
                    .current_template_ipc_client
                    .borrow()
                    .wait_next_request();

                match wait_next_request.get().get_context() {
                    Ok(mut context) => context.set_thread(self_clone.thread_ipc_client.clone()),
                    Err(e) => {
                        tracing::error!("Failed to set thread: {}", e);
                        tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                        self_clone.cancellation_token.cancel();
                        return;
                    }
                }

                let mut wait_next_request_options = match wait_next_request.get().get_options() {
                    Ok(options) => options,
                    Err(e) => {
                        tracing::error!("Failed to get waitNext request options: {}", e);
                        tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                        self_clone.cancellation_token.cancel();
                        return;
                    }
                };

                wait_next_request_options.set_fee_threshold(self_clone.fee_threshold as i64);
                wait_next_request_options.set_timeout(f64::MAX);

                tokio::select! {
                    _ = self_clone.cancellation_token.cancelled() => {
                        tracing::warn!("Exiting mempool change monitoring loop");
                        break;
                    }
                    wait_next_request_response = wait_next_request.send().promise => {
                        match wait_next_request_response {
                            Ok(response) => {
                                let result = match response.get() {
                                    Ok(result) => result,
                                    Err(e) => {
                                        tracing::error!("Failed to get response: {}", e);
                                        tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                        self_clone.cancellation_token.cancel();
                                        break;
                                    }
                                };

                                let new_template_ipc_client = match result.get_result() {
                                    Ok(new_template_ipc_client) => new_template_ipc_client,
                                    Err(e) => {
                                        match e.kind {
                                            capnp::ErrorKind::MessageContainsNullCapabilityPointer => {
                                                tracing::debug!("waitNext timed out (no mempool changes), continuing...");
                                                continue; // Go back to the start of the loop
                                            }
                                            _ => {
                                                tracing::error!("Failed to get new template IPC client: {}", e);
                                                tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                                self_clone.cancellation_token.cancel();
                                                break;
                                            }
                                        }
                                    }
                                };

                                {
                                    let mut current_template_ipc_client_guard = self_clone.current_template_ipc_client.borrow_mut();
                                    *current_template_ipc_client_guard = new_template_ipc_client;
                                }

                                let new_template_data = match self_clone.fetch_template_data().await {
                                    Ok(new_template_data) => new_template_data,
                                    Err(e) => {
                                        tracing::error!("Failed to fetch template data: {:?}", e);
                                        tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                        self_clone.cancellation_token.cancel();
                                        break;
                                    }
                                };

                                let new_prev_hash = new_template_data.get_prev_hash();

                                if new_prev_hash != prev_hash {
                                    info!("⛓️ Chain Tip changed! New prev_hash: {}", new_prev_hash);
                                    prev_hash = new_prev_hash;

                                    // save stale template ids, cleanup and save the new template data
                                    {
                                        let mut template_data_guard = self_clone.template_data.write().await;
                                        let mut stale_template_ids_guard = self_clone.stale_template_ids.write().await;

                                        // save stale template ids
                                        *stale_template_ids_guard = template_data_guard.clone().into_keys().collect::<HashSet<_>>();

                                        // no point in keeping the old templates around
                                        template_data_guard.clear();

                                        // save the new template data
                                        template_data_guard.insert(new_template_data.get_template_id(), new_template_data.clone());
                                    }

                                    // send the future NewTemplate message
                                    let future_template = new_template_data.get_new_template_message(true);

                                    match self_clone.outgoing_template_distribution_message_sender.send(TemplateDistribution::NewTemplate(future_template.clone())).await {
                                        Ok(_) => (),
                                        Err(e) => {
                                            tracing::error!("Failed to send future NewTemplate message: {:?}", e);
                                            tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                            self_clone.cancellation_token.cancel();
                                            break;
                                        }
                                    }

                                    // send the SetNewPrevHash message
                                    let set_new_prev_hash = new_template_data.get_set_new_prev_hash_message();

                                    match self_clone.outgoing_template_distribution_message_sender.send(TemplateDistribution::SetNewPrevHash(set_new_prev_hash.clone())).await {
                                        Ok(_) => (),
                                        Err(e) => {
                                            tracing::error!("Failed to send SetNewPrevHash message: {:?}", e);
                                            tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                            self_clone.cancellation_token.cancel();
                                            break;
                                        }
                                    }
                                } else {
                                    info!("💹 Mempool fees increased! Sending NewTemplate message.");

                                    // send the non-future NewTemplate message
                                    let non_future_template = new_template_data.get_new_template_message(false);

                                    match self_clone.outgoing_template_distribution_message_sender.send(TemplateDistribution::NewTemplate(non_future_template.clone())).await {
                                        Ok(_) => (),
                                        Err(e) => {
                                            tracing::error!("Failed to send future NewTemplate message: {:?}", e);
                                            tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                            self_clone.cancellation_token.cancel();
                                            break;
                                        }
                                    }
                                }

                                // save the new template data
                                self_clone.template_data.write().await.insert(new_template_data.get_template_id(), new_template_data.clone());

                            }
                            Err(e) => {
                                tracing::error!("Failed to get response: {}", e);
                                tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                self_clone.cancellation_token.cancel();
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    fn monitor_request_transaction_data(&self) {
        let self_clone = self.clone();

        tokio::task::spawn_local(async move {
            loop {
                tokio::select! {
                    _ = self_clone.cancellation_token.cancelled() => {
                        tracing::warn!("Exiting mempool change verification loop");
                        break;
                    }
                    Ok(request_transaction_data) = self_clone.request_transaction_data_receiver.recv() => {
                        tracing::debug!("Received: {}", request_transaction_data);

                        // if the template id is stale, send a RequestTransactionDataError message
                        if self_clone.stale_template_ids.read().await.contains(&request_transaction_data.template_id) {
                            let request_transaction_data_error = RequestTransactionDataError {
                                template_id: request_transaction_data.template_id,
                                error_code: "stale-template-id".to_string()                        .to_string()
                                .try_into()
                                .expect("error code must be valid string"),
                            };

                            match self_clone.outgoing_template_distribution_message_sender.send(TemplateDistribution::RequestTransactionDataError(request_transaction_data_error.clone())).await {
                                Ok(_) => (),
                                Err(e) => {
                                    tracing::error!("Failed to send RequestTransactionDataError message: {}", e);
                                    tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                    self_clone.cancellation_token.cancel();
                                    break;
                                }
                            }

                            // continue to the next iteration
                            continue;
                        }

                        let response_message = match self_clone.template_data.read().await.get(&request_transaction_data.template_id) {
                            Some(template_data) => TemplateDistribution::RequestTransactionDataSuccess(template_data.get_request_transaction_data_success_message()),
                            None => {
                                TemplateDistribution::RequestTransactionDataError(RequestTransactionDataError {
                                    template_id: request_transaction_data.template_id,
                                    error_code: "template-id-not-found".to_string()                        .to_string()
                                    .try_into()
                                    .expect("error code must be valid string"),
                                })
                            }
                        };

                        match self_clone.outgoing_template_distribution_message_sender.send(response_message.clone()).await {
                            Ok(_) => (),
                            Err(e) => {
                                tracing::error!("Failed to send message: {}", e);
                                tracing::warn!("Terminating Sv2 Bitcoin Core IPC Connection");
                                self_clone.cancellation_token.cancel();
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    fn monitor_submit_solution(&self) {
        let self_clone = self.clone();

        tokio::task::spawn_local(async move {
            loop {
                tokio::select! {
                    _ = self_clone.cancellation_token.cancelled() => {
                        tracing::warn!("Exiting submit solution loop");
                        break;
                    }
                    Ok(submit_solution) = self_clone.submit_solution_receiver.recv() => {
                        tracing::debug!("Received: {}", submit_solution);

                        let template_data_guard = self_clone.template_data.read().await;

                        let template_data = match template_data_guard.get(&submit_solution.template_id) {
                            Some(template_data) => template_data,
                            None => {
                                tracing::error!("Template data not found for template id: {}", submit_solution.template_id);
                                tracing::warn!("Ignoring SubmitSolution message");
                                continue;
                            }
                        };

                        match template_data.submit_solution(submit_solution, self_clone.thread_ipc_client.clone()).await {
                            Ok(_) => {
                                tracing::info!("Submitted solution successfully");
                            },
                            Err(e) => {
                                tracing::error!("Failed to submit solution: {:?}", e);
                                continue;
                            }
                        }
                    }
                }
            }
        });
    }

    async fn fetch_template_data(&self) -> Result<TemplateData, BitcoinCoreSv2Error> {
        tracing::debug!("Fetching template data over IPC");
        let template_id = self.template_id_factory.fetch_add(1, Ordering::Relaxed);

        // clone the current template IPC client so it's stored in the template data HashMap
        // this is important in case we need to submit a solution relative to this specific template
        // by the time self.current_template_ipc_client might have already changed
        let template_ipc_client = self.current_template_ipc_client.borrow().clone();

        let mut template_block_request = self
            .current_template_ipc_client
            .borrow()
            .get_block_request();
        template_block_request
            .get()
            .get_context()?
            .set_thread(self.thread_ipc_client.clone());

        let template_block_bytes = template_block_request
            .send()
            .promise
            .await?
            .get()?
            .get_result()?
            .to_vec();

        // Deserialize the complete block template from Bitcoin Core's serialization format
        let block: Block = deserialize(&template_block_bytes)?;

        // Create the template data structure
        let template_data = TemplateData::new(template_id, block, template_ipc_client);

        Ok(template_data)
    }
}
