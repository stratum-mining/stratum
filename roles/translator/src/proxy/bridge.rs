use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    channel_logic::channel_factory::{ExtendedChannelKind, ProxyExtendedChannelFactory, Share},
    job_creator::JobsCreators,
    mining_sv2::{
        ExtendedExtranonce, NewExtendedMiningJob, SetCustomMiningJob, SetNewPrevHash,
        SubmitSharesExtended, Target,
    },
    parsers::Mining,
    template_distribution_sv2::{
        NewTemplate, SetNewPrevHash as SetNewPrevHashTemplate, SubmitSolution,
    },
    utils::{GroupId, Id, Mutex},
};
use std::sync::Arc;
use tokio::sync::broadcast;
use v1::{client_to_server::Submit, server_to_client, utils::HexU32Be};

use crate::{
    downstream_sv1::{DownstreamMessages, SetDownstreamTarget, SubmitShareWithChannelId},
    error::Error::{self, PoisonLock},
    status, ProxyResult,
};
use error_handling::handle_result;
use roles_logic_sv2::{
    bitcoin::TxOut, channel_logic::channel_factory::OnNewShare, Error as RolesLogicError,
};
use tracing::{debug, error, info};

/// Bridge between the SV2 `Upstream` and SV1 `Downstream` responsible for the following messaging
/// translation:
/// 1. SV1 `mining.submit` -> SV2 `SubmitSharesExtended`
/// 2. SV2 `SetNewPrevHash` + `NewExtendedMiningJob` -> SV1 `mining.notify`
#[derive(Debug)]
pub struct Bridge {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    rx_sv1_downstream: Receiver<DownstreamMessages>,
    /// Sends SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages to
    /// the `Upstream`.
    tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` to be translated (along with a
    /// SV2 `NewExtendedMiningJob` message) to a SV1 `mining.submit` for the `Downstream`.
    rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream` to be translated (along
    /// with a SV2 `SetNewPrevHash` message) to a SV1 `mining.submit` to be sent to the
    /// `Downstream`.
    rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    /// Sends SV1 `mining.notify` message (translated from the SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages stored in the `NextMiningNotify`) to the `Downstream`.
    tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    /// Allows the bridge the ability to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
    /// Stores the most recent SV1 `mining.notify` values to be sent to the `Downstream` upon
    /// receiving a new SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages **before** any
    /// Downstream role connects to the proxy.
    ///
    /// Once the proxy establishes a connection with the SV2 Upstream role, it immediately receives
    /// a SV2 `SetNewPrevHash` and `NewExtendedMiningJob` message. This happens before the
    /// connection to the Downstream role(s) occur. The `last_notify` member fields allows these
    /// first notify values to be relayed to the `Downstream` once a Downstream role connects. Once
    /// a Downstream role connects and receives the first notify values, this member field is no
    /// longer used.
    last_notify: Option<server_to_client::Notify<'static>>,
    pub(self) channel_factory: ProxyExtendedChannelFactory,
    future_jobs: Vec<NewExtendedMiningJob<'static>>,
    last_p_hash: Option<SetNewPrevHash<'static>>,
    target: Arc<Mutex<Vec<u8>>>,
    first_ph_received: bool,
    pool_output_is_set: bool,
    request_ids: Id,
    solution_sender: Option<Sender<SubmitSolution<'static>>>,
    last_job_id: u32,
    has_negotiator: bool,
    first_job_handled: bool,
    //up_to_down_job_ids: std::collections::hash_map::HashMap<u32, Vec<u32>>,
    withhold: bool,
}

#[derive(Debug, Clone)]
pub enum UpstreamKind {
    Standard,
    WithNegotiator {
        recv_tp: Receiver<(NewTemplate<'static>, Vec<u8>)>,
        recv_ph: Receiver<(SetNewPrevHashTemplate<'static>, Vec<u8>)>,
        recv_coinbase_out: Receiver<(Vec<TxOut>, Vec<u8>)>,
        send_mining_job: Sender<SetCustomMiningJob<'static>>,
        send_solution: Sender<SubmitSolution<'static>>,
    },
}

impl UpstreamKind {
    pub fn has_negotiator(&self) -> bool {
        match self {
            UpstreamKind::Standard => false,
            UpstreamKind::WithNegotiator { .. } => true,
        }
    }
}

impl Bridge {
    #[allow(clippy::too_many_arguments)]
    /// Instantiate a new `Bridge`.
    pub fn new(
        rx_sv1_downstream: Receiver<DownstreamMessages>,
        tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
        rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        extranonces: ExtendedExtranonce,
        target: Arc<Mutex<Vec<u8>>>,
        up_id: u32,
        upstream_kind: UpstreamKind,
        withhold: bool,
    ) -> Arc<Mutex<Self>> {
        let ids = Arc::new(Mutex::new(GroupId::new()));
        let share_per_min = 1.0;
        let upstream_target: [u8; 32] =
            target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
        let upstream_target: Target = upstream_target.into();
        let (job_creator, kind, solution_sender) = match upstream_kind {
            UpstreamKind::Standard => (None, ExtendedChannelKind::Proxy { upstream_target }, None),
            UpstreamKind::WithNegotiator {
                ref send_solution, ..
            } => (
                Some(JobsCreators::new(extranonces.get_len() as u8)),
                ExtendedChannelKind::ProxyJn { upstream_target },
                Some(send_solution.clone()),
            ),
        };
        let self_ = Arc::new(Mutex::new(Self {
            rx_sv1_downstream,
            tx_sv2_submit_shares_ext,
            rx_sv2_set_new_prev_hash,
            rx_sv2_new_ext_mining_job,
            tx_sv1_notify,
            tx_status,
            last_notify: None,
            channel_factory: ProxyExtendedChannelFactory::new(
                ids,
                extranonces,
                job_creator,
                share_per_min,
                kind,
                None,
                up_id,
            ),
            future_jobs: vec![],
            last_p_hash: None,
            target,
            first_ph_received: false,
            request_ids: Id::new(),
            pool_output_is_set: false,
            solution_sender,
            last_job_id: 0,
            has_negotiator: upstream_kind.has_negotiator(),
            first_job_handled: false,
            withhold,
        }));
        match upstream_kind {
            UpstreamKind::Standard => (),
            UpstreamKind::WithNegotiator {
                recv_tp,
                recv_ph,
                recv_coinbase_out,
                send_mining_job,
                ..
            } => {
                // open a channel so that jobs are created and last notify is updated also if no
                // dowsntream connected
                self_
                    .safe_lock(|s| {
                        s.channel_factory.new_extended_channel(0, 1.0, 8);
                    })
                    .unwrap();
                Self::start_receiving_pool_coinbase_outs(self_.clone(), recv_coinbase_out);
                Self::start_receiving_new_template(self_.clone(), recv_tp, send_mining_job.clone());
                Self::start_receiving_new_prev_hash(self_.clone(), recv_ph, send_mining_job);
            }
        };
        self_
    }

    pub fn start_receiving_pool_coinbase_outs(
        self_mutex: Arc<Mutex<Self>>,
        recv: Receiver<(Vec<TxOut>, Vec<u8>)>,
    ) {
        let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
        task::spawn(async move {
            while let Ok((outs, _id)) = recv.recv().await {
                // TODO assuming that only one coinbase is negotiated with the pool not handling
                // different tokens, in order to do that we can use the out hashmap:
                // self.tx_outs.insert(id, outs)
                let to_handle = self_mutex
                    .safe_lock(|s| {
                        s.channel_factory.update_pool_outputs(outs);
                        s.pool_output_is_set = true;
                    })
                    .map_err(|_| PoisonLock);
                handle_result!(tx_status, to_handle);
            }
        });
    }

    pub fn start_receiving_new_template(
        self_mutex: Arc<Mutex<Self>>,
        new_template_reciver: Receiver<(NewTemplate<'static>, Vec<u8>)>,
        send_mining_job: Sender<SetCustomMiningJob<'static>>,
    ) {
        task::spawn(async move {
            debug!("Bridge waiting to receive first prev hash and first pool outputs");
            while !self_mutex.safe_lock(|s| (s.pool_output_is_set)).unwrap() {
                tokio::task::yield_now().await;
            }
            debug!("Bridge received first prev hash and first pool outputs");
            let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
            loop {
                let (mut message_new_template, token): (NewTemplate, Vec<u8>) =
                    handle_result!(tx_status.clone(), new_template_reciver.recv().await);
                let partial = self_mutex
                    .safe_lock(|a| a.channel_factory.on_new_template(&mut message_new_template))
                    .map_err(|_| PoisonLock);
                let partial = handle_result!(tx_status.clone(), partial);
                let (channel_id_to_new_job_msg, custom_job, _) =
                    handle_result!(tx_status.clone(), partial);
                if let Some(custom_job) = custom_job {
                    let req_id = self_mutex.safe_lock(|s| s.request_ids.next()).unwrap();
                    let custom_mining_job = SetCustomMiningJob {
                        channel_id: self_mutex
                            .safe_lock(|s| s.channel_factory.get_this_channel_id())
                            .unwrap(),
                        request_id: req_id,
                        version: custom_job.version,
                        prev_hash: custom_job.prev_hash,
                        min_ntime: custom_job.min_ntime,
                        nbits: custom_job.nbits,
                        coinbase_tx_version: custom_job.coinbase_tx_version,
                        coinbase_prefix: custom_job.coinbase_prefix.clone(),
                        coinbase_tx_input_n_sequence: custom_job.coinbase_tx_input_n_sequence,
                        coinbase_tx_value_remaining: custom_job.coinbase_tx_value_remaining,
                        coinbase_tx_outputs: custom_job.coinbase_tx_outputs.clone(),
                        coinbase_tx_locktime: custom_job.coinbase_tx_locktime,
                        merkle_path: custom_job.merkle_path,
                        extranonce_size: custom_job.extranonce_size,
                        future_job: message_new_template.future_template,
                        // token come from a valid Sv2 message so it can always be serialized safe
                        // unwrap
                        token: token.try_into().unwrap(),
                    };
                    handle_result!(
                        tx_status.clone(),
                        send_mining_job.send(custom_mining_job).await
                    );
                }

                let (tx_sv1_notify, tx_status) = self_mutex
                    .safe_lock(|s| (s.tx_sv1_notify.clone(), s.tx_status.clone()))
                    .unwrap();
                for (id, job) in channel_id_to_new_job_msg {
                    let should_ignore_first_channel = self_mutex
                        .safe_lock(|s| s.has_negotiator && s.first_job_handled)
                        .map_err(|_| PoisonLock);
                    // If we have negotiator the first job is for an inexistend channel created
                    // only for initialize the channel factory when no dowstream are connected
                    if handle_result!(tx_status.clone(), should_ignore_first_channel) && id == 1 {
                        continue;
                    }
                    self_mutex
                        .safe_lock(|s| s.first_job_handled = true)
                        .unwrap();
                    if let Mining::NewExtendedMiningJob(job) = job {
                        handle_result!(
                            tx_status.clone(),
                            Self::handle_new_extended_mining_job_(
                                self_mutex.clone(),
                                job,
                                tx_sv1_notify.clone(),
                            )
                            .await
                        )
                    }
                }
                crate::upstream_sv2::upstream::IS_NEW_TEMPLATE_HANDLED
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }

    pub fn start_receiving_new_prev_hash(
        self_mutex: Arc<Mutex<Self>>,
        prev_hash_reciver: Receiver<(SetNewPrevHashTemplate<'static>, Vec<u8>)>,
        send_mining_job: Sender<SetCustomMiningJob<'static>>,
    ) {
        task::spawn(async move {
            let (tx_sv1_notify, tx_status) = self_mutex
                .safe_lock(|s| (s.tx_sv1_notify.clone(), s.tx_status.clone()))
                .unwrap();
            loop {
                let (message_prev_hash, token) = prev_hash_reciver.recv().await.unwrap();
                let custom = self_mutex
                    .safe_lock(|a| {
                        a.channel_factory
                            .on_new_prev_hash_from_tp(&message_prev_hash)
                    })
                    .unwrap();
                self_mutex
                    .safe_lock(|s| s.first_ph_received = true)
                    .unwrap();

                let new_p_hash = SetNewPrevHash {
                    channel_id: 0,
                    job_id: match custom {
                        Ok(Some((_, id))) => id,
                        _ => 0,
                    },
                    prev_hash: message_prev_hash.prev_hash,
                    min_ntime: message_prev_hash.header_timestamp,
                    nbits: message_prev_hash.n_bits,
                };

                if let Ok(Some((custom_job, _))) = custom {
                    let req_id = self_mutex.safe_lock(|s| s.request_ids.next()).unwrap();
                    let custom_mining_job = SetCustomMiningJob {
                        channel_id: self_mutex
                            .safe_lock(|s| s.channel_factory.get_this_channel_id())
                            .unwrap(),
                        request_id: req_id,
                        version: custom_job.version,
                        prev_hash: custom_job.prev_hash,
                        min_ntime: custom_job.min_ntime,
                        nbits: custom_job.nbits,
                        coinbase_tx_version: custom_job.coinbase_tx_version,
                        coinbase_prefix: custom_job.coinbase_prefix.clone(),
                        coinbase_tx_input_n_sequence: custom_job.coinbase_tx_input_n_sequence,
                        coinbase_tx_value_remaining: custom_job.coinbase_tx_value_remaining,
                        coinbase_tx_outputs: custom_job.coinbase_tx_outputs.clone(),
                        coinbase_tx_locktime: custom_job.coinbase_tx_locktime,
                        merkle_path: custom_job.merkle_path,
                        extranonce_size: custom_job.extranonce_size,
                        future_job: false,
                        // token come from a valid Sv2 message so it can always be serialized safe
                        // unwrap
                        token: token.try_into().unwrap(),
                    };
                    let tx_status = self_mutex.safe_lock(|s| s.tx_status.clone()).unwrap();
                    handle_result!(tx_status, send_mining_job.send(custom_mining_job).await);
                }
                handle_result!(
                    tx_status.clone(),
                    Self::handle_new_prev_hash_(
                        self_mutex.clone(),
                        new_p_hash,
                        tx_sv1_notify.clone(),
                    )
                    .await
                );
            }
        });
    }

    pub fn on_new_sv1_connection(
        &mut self,
        hash_rate: f32,
    ) -> ProxyResult<'static, OpenSv1Downstream> {
        match self.channel_factory.new_extended_channel(0, hash_rate, 0) {
            Some(messages) => {
                for message in messages {
                    match message {
                        Mining::OpenExtendedMiningChannelSuccess(success) => {
                            let extranonce = success.extranonce_prefix.to_vec();
                            let extranonce2_len = success.extranonce_size;
                            self.target
                                .safe_lock(|t| *t = success.target.to_vec())
                                .map_err(|_e| PoisonLock)?;
                            return Ok(OpenSv1Downstream {
                                channel_id: success.channel_id,
                                last_notify: self.last_notify.clone(),
                                extranonce,
                                target: self.target.clone(),
                                extranonce2_len,
                            });
                        }
                        Mining::OpenMiningChannelError(_) => todo!(),
                        Mining::SetNewPrevHash(_) => (),
                        Mining::NewExtendedMiningJob(_) => (),
                        _ => unreachable!(),
                    }
                }
            }
            None => {
                return Err(Error::SubprotocolMining(
                    "Bridge: failed to open new extended channel".to_string(),
                ))
            }
        };
        Err(Error::SubprotocolMining(
            "Bridge: Invalid mining message when opening downstream connection".to_string(),
        ))
    }

    /// Starts the tasks that receive SV1 and SV2 messages to be translated and sent to their
    /// respective roles.
    pub fn start(self_: Arc<Mutex<Self>>) {
        Self::handle_new_prev_hash(self_.clone());
        Self::handle_new_extended_mining_job(self_.clone());
        Self::handle_downstream_messages(self_);
    }

    /// Receives a `DownstreamMessages` message from the `Downstream`, handles based on the
    /// variant received.
    fn handle_downstream_messages(self_: Arc<Mutex<Self>>) {
        let (rx_sv1_downstream, tx_status) = self_
            .safe_lock(|s| (s.rx_sv1_downstream.clone(), s.tx_status.clone()))
            .unwrap();
        task::spawn(async move {
            loop {
                let msg = handle_result!(tx_status, rx_sv1_downstream.clone().recv().await);

                match msg {
                    DownstreamMessages::SubmitShares(share) => {
                        handle_result!(
                            tx_status,
                            Self::handle_submit_shares(self_.clone(), share).await
                        );
                    }
                    DownstreamMessages::SetDownstreamTarget(new_target) => {
                        handle_result!(
                            tx_status,
                            Self::handle_update_downstream_target(self_.clone(), new_target)
                        );
                    }
                };
            }
        });
    }
    /// receives a `SetDownstreamTarget` and updates the downstream target for the channel
    fn handle_update_downstream_target(
        self_: Arc<Mutex<Self>>,
        new_target: SetDownstreamTarget,
    ) -> ProxyResult<'static, ()> {
        self_
            .safe_lock(|b| {
                b.channel_factory
                    .update_target_for_channel(new_target.channel_id, new_target.new_target);
            })
            .map_err(|_| PoisonLock)?;
        Ok(())
    }
    /// receives a `SubmitShareWithChannelId` and validates the shares and sends to `Upstream` if
    /// the share meets the upstream target
    async fn handle_submit_shares(
        self_: Arc<Mutex<Self>>,
        share: SubmitShareWithChannelId,
    ) -> ProxyResult<'static, ()> {
        let withhold = self_.safe_lock(|s| s.withhold).map_err(|_| PoisonLock)?;
        let (tx_sv2_submit_shares_ext, target_mutex, tx_status) = self_
            .safe_lock(|s| {
                (
                    s.tx_sv2_submit_shares_ext.clone(),
                    s.target.clone(),
                    s.tx_status.clone(),
                )
            })
            .map_err(|_| PoisonLock)?;
        let upstream_target: [u8; 32] = target_mutex
            .safe_lock(|t| t.clone())
            .map_err(|_| PoisonLock)?
            .try_into()?;
        let mut upstream_target: Target = upstream_target.into();
        self_
            .safe_lock(|s| s.channel_factory.set_target(&mut upstream_target))
            .map_err(|_| PoisonLock)?;

        let sv2_submit = self_
            .safe_lock(|s| {
                s.translate_submit(share.channel_id, share.share, share.version_rolling_mask)
            })
            .map_err(|_| PoisonLock)??;
        let res = self_
            .safe_lock(|s| {
                s.channel_factory.set_target(&mut upstream_target);
                s.channel_factory.on_submit_shares_extended(sv2_submit)
            })
            .map_err(|_| PoisonLock);

        match res {
            Ok(Ok(OnNewShare::SendErrorDownstream(e))) => {
                error!(
                    "Submit share error {:?}",
                    std::str::from_utf8(&e.error_code.to_vec()[..])
                );
                error!("Make sure to set `min_individual_miner_hashrate` in the config file");
            }
            Ok(Ok(OnNewShare::SendSubmitShareUpstream(share))) => {
                info!("SHARE MEETS TARGET");
                match share {
                    Share::Extended(share) => {
                        tx_sv2_submit_shares_ext.send(share).await?;
                    }
                    // We are in an extended channel shares are extended
                    Share::Standard(_) => unreachable!(),
                }
            }
            // We are in an extended channel this variant is group channle only
            Ok(Ok(OnNewShare::RelaySubmitShareUpstream)) => unreachable!(),
            Ok(Ok(OnNewShare::ShareMeetDownstreamTarget)) => {
                info!("SHARE MEETS DOWNSTREAM TARGET");
            }
            Ok(Ok(OnNewShare::ShareMeetBitcoinTarget((share, Some(template_id), coinbase)))) => {
                match share {
                    Share::Extended(s) => {
                        info!("SHARE MEETS BITCOIN TARGET");
                        let solution_sender = self_
                            .safe_lock(|s| s.solution_sender.clone())
                            .map_err(|_| PoisonLock)?
                            .unwrap();
                        let solution = SubmitSolution {
                            template_id,
                            version: s.version,
                            header_timestamp: s.ntime,
                            header_nonce: s.nonce,
                            coinbase_tx: coinbase.try_into()?,
                        };
                        // The below channel should never be full is ok to block
                        solution_sender.send_blocking(solution).unwrap();
                        if !withhold {
                            tx_sv2_submit_shares_ext.send(s).await?;
                        }
                    }
                    // We are in an extended channel shares are extended
                    Share::Standard(_) => unreachable!(),
                }
            }
            // When we have a ShareMeetBitcoinTarget it means that the proxy know the bitcoin
            // target that means that the proxy must have JN capabilities that means that the
            // second tuple elements can not be None but must be Some(template_id)
            Ok(Ok(OnNewShare::ShareMeetBitcoinTarget(..))) => unreachable!(),
            Ok(Err(e)) => error!("Error: {:?}", e),
            Err(e) => {
                let _ = tx_status
                    .send(status::Status {
                        state: status::State::BridgeShutdown(e),
                    })
                    .await;
            }
        }
        Ok(())
    }

    /// Translates a SV1 `mining.submit` message to a SV2 `SubmitSharesExtended` message.
    fn translate_submit(
        &self,
        channel_id: u32,
        sv1_submit: Submit,
        version_rolling_mask: Option<HexU32Be>,
    ) -> ProxyResult<'static, SubmitSharesExtended<'static>> {
        let last_version = self
            .channel_factory
            .last_valid_job_version()
            .ok_or(Error::RolesSv2Logic(RolesLogicError::NoValidJob))?;
        let version = match (sv1_submit.version_bits, version_rolling_mask) {
            // regarding version masking see https://github.com/slushpool/stratumprotocol/blob/master/stratum-extensions.mediawiki#changes-in-request-miningsubmit
            (Some(vb), Some(mask)) => (last_version & !mask.0) | (vb.0 & mask.0),
            (None, None) => last_version,
            _ => return Err(Error::V1Protocol(v1::error::Error::InvalidSubmission)),
        };
        let mining_device_extranonce: Vec<u8> = sv1_submit.extra_nonce2.into();
        let extranonce2 = mining_device_extranonce;
        Ok(SubmitSharesExtended {
            channel_id,
            // I put 0 below cause sequence_number is not what should be TODO
            sequence_number: 0,
            job_id: sv1_submit.job_id.parse::<u32>()?,
            nonce: sv1_submit.nonce.0,
            ntime: sv1_submit.time.0,
            version,
            extranonce: extranonce2.try_into()?,
        })
    }

    async fn handle_new_prev_hash_(
        self_: Arc<Mutex<Self>>,
        sv2_set_new_prev_hash: SetNewPrevHash<'static>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    ) -> Result<(), Error<'static>> {
        while !crate::upstream_sv2::upstream::IS_NEW_JOB_HANDLED
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            tokio::task::yield_now().await;
        }
        self_
            .safe_lock(|s| s.last_p_hash = Some(sv2_set_new_prev_hash.clone()))
            .map_err(|_| PoisonLock)?;

        let on_new_prev_hash_res = self_
            .safe_lock(|s| {
                s.channel_factory
                    .on_new_prev_hash(sv2_set_new_prev_hash.clone())
            })
            .map_err(|_| PoisonLock)?;
        on_new_prev_hash_res?;

        let mut future_jobs = self_
            .safe_lock(|s| {
                let future_jobs = s.future_jobs.clone();
                s.future_jobs = vec![];
                future_jobs
            })
            .map_err(|_| PoisonLock)?;

        let mut match_a_future_job = false;
        while let Some(job) = future_jobs.pop() {
            if job.job_id == sv2_set_new_prev_hash.job_id {
                let j_id = job.job_id;
                // Create the mining.notify to be sent to the Downstream.
                let notify = crate::proxy::next_mining_notify::create_notify(
                    sv2_set_new_prev_hash.clone(),
                    job,
                );

                // Get the sender to send the mining.notify to the Downstream
                tx_sv1_notify.send(notify.clone())?;
                match_a_future_job = true;
                self_
                    .safe_lock(|s| {
                        s.last_notify = Some(notify);
                        s.last_job_id = j_id;
                    })
                    .map_err(|_| PoisonLock)?;
                break;
            }
        }
        if !match_a_future_job {
            debug!("No future jobs for {:?}", sv2_set_new_prev_hash);
        }
        Ok(())
    }

    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` and creates a SV1
    /// `mining.notify` message (in conjunction with a previously received SV2
    /// `NewExtendedMiningJob` message) which is sent to the `Downstream`. The protocol requires
    /// that before every received `SetNewPrevHash`, a `NewExtendedMiningJob` with a
    /// corresponding `job_id` has already been received. If this is not the case, an error has
    /// occurred on the Upstream pool role and the connection will close.
    fn handle_new_prev_hash(self_: Arc<Mutex<Self>>) {
        let (tx_sv1_notify, rx_sv2_set_new_prev_hash, tx_status) = self_
            .safe_lock(|s| {
                (
                    s.tx_sv1_notify.clone(),
                    s.rx_sv2_set_new_prev_hash.clone(),
                    s.tx_status.clone(),
                )
            })
            .unwrap();
        debug!("Starting handle_new_prev_hash task");
        task::spawn(async move {
            loop {
                // Receive `SetNewPrevHash` from `Upstream`
                let sv2_set_new_prev_hash: SetNewPrevHash =
                    handle_result!(tx_status, rx_sv2_set_new_prev_hash.clone().recv().await);
                debug!(
                    "handle_new_prev_hash job_id: {:?}",
                    &sv2_set_new_prev_hash.job_id
                );
                handle_result!(
                    tx_status.clone(),
                    Self::handle_new_prev_hash_(
                        self_.clone(),
                        sv2_set_new_prev_hash,
                        tx_sv1_notify.clone(),
                    )
                    .await
                )
            }
        });
    }

    async fn handle_new_extended_mining_job_(
        self_: Arc<Mutex<Self>>,
        sv2_new_extended_mining_job: NewExtendedMiningJob<'static>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    ) -> Result<(), Error<'static>> {
        // convert to non segwit jobs so we dont have to depend if miner's support segwit or not
        self_
            .safe_lock(|s| {
                s.channel_factory
                    .on_new_extended_mining_job(sv2_new_extended_mining_job.as_static().clone())
            })
            .map_err(|_| PoisonLock)??;

        // If future_job=true, this job is meant for a future SetNewPrevHash that the proxy
        // has yet to receive. Insert this new job into the job_mapper .
        if sv2_new_extended_mining_job.is_future() {
            self_
                .safe_lock(|s| s.future_jobs.push(sv2_new_extended_mining_job.clone()))
                .map_err(|_| PoisonLock)?;
            Ok(())

        // If future_job=false, this job is meant for the current SetNewPrevHash.
        } else {
            let last_p_hash_option = self_
                .safe_lock(|s| s.last_p_hash.clone())
                .map_err(|_| PoisonLock)?;

            // last_p_hash is an Option<SetNewPrevHash> so we need to map to the correct error type to be handled
            let last_p_hash = last_p_hash_option.ok_or(Error::RolesSv2Logic(
                RolesLogicError::JobIsNotFutureButPrevHashNotPresent,
            ))?;

            let j_id = sv2_new_extended_mining_job.job_id;
            // Create the mining.notify to be sent to the Downstream.
            let notify = crate::proxy::next_mining_notify::create_notify(
                last_p_hash,
                sv2_new_extended_mining_job.clone(),
            );
            // Get the sender to send the mining.notify to the Downstream
            tx_sv1_notify.send(notify.clone())?;
            self_
                .safe_lock(|s| {
                    s.last_notify = Some(notify);
                    s.last_job_id = j_id;
                })
                .map_err(|_| PoisonLock)?;
            Ok(())
        }
    }

    /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream`. If `future_job=true`,
    /// this job is intended for a future SV2 `SetNewPrevHash` that has yet to be received. This
    /// job is stored until a SV2 `SetNewPrevHash` message with a corresponding `job_id` is
    /// received. If `future_job=false`, this job is intended for the SV2 `SetNewPrevHash` that is
    /// currently being mined on. In this case, a SV1 `mining.notify` is created and is sent to the
    /// `Downstream`. If `future_job=false` but this job's `job_id` does not match the current SV2
    /// `SetNewPrevHash` `job_id`, an error has occurred on the Upstream pool role and the
    /// connection will close.
    fn handle_new_extended_mining_job(self_: Arc<Mutex<Self>>) {
        let (tx_sv1_notify, rx_sv2_new_ext_mining_job, tx_status) = self_
            .safe_lock(|s| {
                (
                    s.tx_sv1_notify.clone(),
                    s.rx_sv2_new_ext_mining_job.clone(),
                    s.tx_status.clone(),
                )
            })
            .unwrap();
        debug!("Starting handle_new_extended_mining_job task");
        task::spawn(async move {
            loop {
                // Receive `NewExtendedMiningJob` from `Upstream`
                let sv2_new_extended_mining_job: NewExtendedMiningJob = handle_result!(
                    tx_status.clone(),
                    rx_sv2_new_ext_mining_job.clone().recv().await
                );
                debug!(
                    "handle_new_extended_mining_job job_id: {:?}",
                    &sv2_new_extended_mining_job.job_id
                );
                handle_result!(
                    tx_status,
                    Self::handle_new_extended_mining_job_(
                        self_.clone(),
                        sv2_new_extended_mining_job,
                        tx_sv1_notify.clone(),
                    )
                    .await
                );
                crate::upstream_sv2::upstream::IS_NEW_JOB_HANDLED
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }
}
pub struct OpenSv1Downstream {
    pub channel_id: u32,
    pub last_notify: Option<server_to_client::Notify<'static>>,
    pub extranonce: Vec<u8>,
    pub target: Arc<Mutex<Vec<u8>>>,
    pub extranonce2_len: u16,
}

#[cfg(test)]
mod test {
    use super::*;
    use async_channel::{bounded, unbounded};
    use roles_logic_sv2::{bitcoin::util::psbt::serialize::Serialize, job_creator::Decodable};

    pub mod test_utils {
        use super::*;

        pub struct BridgeInterface {
            pub tx_sv1_submit: Sender<DownstreamMessages>,
            pub rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
            pub tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
            pub tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
            pub rx_sv1_notify: broadcast::Receiver<server_to_client::Notify<'static>>,
        }
        pub struct BridgeNegotiatorInterface {
            pub send_tp: Sender<(NewTemplate<'static>, Vec<u8>)>,
            pub send_ph: Sender<(SetNewPrevHashTemplate<'static>, Vec<u8>)>,
            pub send_cb_out: Sender<(Vec<TxOut>, Vec<u8>)>,
            pub recv_mining_job: Receiver<SetCustomMiningJob<'static>>,
            pub recv_solution: Receiver<SubmitSolution<'static>>,
        }

        pub fn create_upstream_kind_neg() -> (UpstreamKind, BridgeNegotiatorInterface) {
            let (send_tp, recv_tp) = unbounded();
            let (send_ph, recv_ph) = unbounded();
            let (send_cb_out, recv_coinbase_out) = unbounded();
            let (send_mining_job, recv_mining_job) = unbounded();
            let (send_solution, recv_solution) = unbounded();

            let upstream_kind = UpstreamKind::WithNegotiator {
                recv_tp,
                recv_ph,
                recv_coinbase_out,
                send_mining_job,
                send_solution,
            };

            let jn_interface = test_utils::BridgeNegotiatorInterface {
                send_tp,
                send_ph,
                send_cb_out,
                recv_mining_job,
                recv_solution,
            };
            (upstream_kind, jn_interface)
        }

        pub fn create_bridge(
            upstream_kind: UpstreamKind,
            extranonces: ExtendedExtranonce,
        ) -> (Arc<Mutex<Bridge>>, BridgeInterface) {
            let (tx_sv1_submit, rx_sv1_submit) = bounded(1);
            let (tx_sv2_submit_shares_ext, rx_sv2_submit_shares_ext) = bounded(1);
            let (tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(1);
            let (tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(1);
            let (tx_sv1_notify, rx_sv1_notify) = broadcast::channel(1);
            let (tx_status, _rx_status) = bounded(1);
            let upstream_target = vec![
                0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0,
            ];
            let interface = BridgeInterface {
                tx_sv1_submit,
                rx_sv2_submit_shares_ext,
                tx_sv2_set_new_prev_hash,
                tx_sv2_new_ext_mining_job,
                rx_sv1_notify,
            };

            let b = Bridge::new(
                rx_sv1_submit,
                tx_sv2_submit_shares_ext,
                rx_sv2_set_new_prev_hash,
                rx_sv2_new_ext_mining_job,
                tx_sv1_notify,
                status::Sender::Bridge(tx_status),
                extranonces,
                Arc::new(Mutex::new(upstream_target)),
                1,
                upstream_kind,
                false,
            );
            (b, interface)
        }

        pub fn create_sv1_submit(job_id: u32) -> Submit<'static> {
            Submit {
                user_name: "test_user".to_string(),
                job_id: job_id.to_string(),
                extra_nonce2: v1::utils::Extranonce::try_from([0; 32].to_vec()).unwrap(),
                time: v1::utils::HexU32Be(1),
                nonce: v1::utils::HexU32Be(1),
                version_bits: None,
                id: 0,
            }
        }
    }

    #[test]
    fn test_version_bits_insert() {
        use roles_logic_sv2::bitcoin::hashes::Hash;
        let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..16);
        let (bridge, _) = test_utils::create_bridge(UpstreamKind::Standard, extranonces);
        bridge
            .safe_lock(|bridge| {
                let channel_id = 1;
                let out_id = roles_logic_sv2::bitcoin::hashes::sha256d::Hash::from_slice(&[
                    0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0,
                ])
                .unwrap();
                let p_out = roles_logic_sv2::bitcoin::OutPoint {
                    txid: roles_logic_sv2::bitcoin::Txid::from_hash(out_id),
                    vout: 0xffff_ffff,
                };
                let in_ = roles_logic_sv2::bitcoin::TxIn {
                    previous_output: p_out,
                    script_sig: vec![89_u8; 16].into(),
                    sequence: 0,
                    witness: vec![].into(),
                };
                let tx = roles_logic_sv2::bitcoin::Transaction {
                    version: 1,
                    lock_time: 0,
                    input: vec![in_],
                    output: vec![],
                };
                let tx = tx.serialize();
                let _down = bridge
                    .channel_factory
                    .add_standard_channel(0, 10_000_000_000.0, true, 1)
                    .unwrap();
                let prev_hash = SetNewPrevHash {
                    channel_id,
                    job_id: 0,
                    prev_hash: [
                        3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
                        3, 3, 3, 3, 3, 3, 3,
                    ]
                    .into(),
                    min_ntime: 989898,
                    nbits: 9,
                };
                bridge.channel_factory.on_new_prev_hash(prev_hash).unwrap();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as u32;
                let new_mining_job = NewExtendedMiningJob {
                    channel_id,
                    job_id: 0,
                    min_ntime: binary_sv2::Sv2Option::new(Some(now)),
                    version: 0b0000_0000_0000_0000,
                    version_rolling_allowed: false,
                    merkle_path: vec![].into(),
                    coinbase_tx_prefix: tx[0..42].to_vec().try_into().unwrap(),
                    coinbase_tx_suffix: tx[58..].to_vec().try_into().unwrap(),
                };
                bridge
                    .channel_factory
                    .on_new_extended_mining_job(new_mining_job.clone())
                    .unwrap();

                // pass sv1_submit into Bridge::translate_submit
                let sv1_submit = test_utils::create_sv1_submit(0);
                let sv2_message = bridge
                    .translate_submit(channel_id, sv1_submit, None)
                    .unwrap();
                // assert sv2 message equals sv1 with version bits added
                assert_eq!(
                    new_mining_job.version, sv2_message.version,
                    "Version bits were not inserted for non version rolling sv1 message"
                );
            })
            .unwrap();
    }
    use std::convert::TryInto;

    #[tokio::test]
    async fn get_two_jobs_within_the_same_p_hash_jn() {
        let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..32);
        let tx_out_ = vec![
            0_u8, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
            222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
            139, 235, 216, 54, 151, 78, 140, 249,
        ];
        let tx_out_1 = [
            0, 242, 5, 42, 1, 0, 0, 0, 25, 118, 169, 20, 85, 162, 233, 20, 174, 185, 114, 155, 76,
            210, 101, 36, 140, 182, 122, 134, 94, 174, 149, 253, 136, 172,
        ];

        // Create bridge
        let (upstream_kind, neg_interface) = test_utils::create_upstream_kind_neg();
        // interface must not dropped or channels will broks
        let (bridge, _interface) = test_utils::create_bridge(upstream_kind, extranonces);
        let tx_out_1 = roles_logic_sv2::bitcoin::TxOut::consensus_decode(&tx_out_1[..]).unwrap();

        // Send first cb out
        neg_interface
            .send_cb_out
            .send((vec![tx_out_1], vec![0; 4]))
            .await
            .unwrap();

        let first_templ = NewTemplate {
            template_id: 78,
            future_template: true,
            version: 805306368,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 65, 1, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 1250000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: tx_out_.try_into().unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let first_ph = SetNewPrevHashTemplate {
            template_id: 78,
            prev_hash: vec![0; 32].try_into().unwrap(),
            header_timestamp: 98,
            n_bits: 67,
            target: vec![0; 32].try_into().unwrap(),
        };

        // Send first template and prev hash
        neg_interface
            .send_tp
            .send((first_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((first_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Open downstream connection
        bridge
            .safe_lock(|b| b.on_new_sv1_connection(100_000.0))
            .unwrap()
            .unwrap();

        let mut second_templ = first_templ.clone();
        second_templ.template_id = 90;

        let mut second_ph = first_ph.clone();
        second_ph.template_id = 90;

        // Send second template and prev hash
        neg_interface
            .send_tp
            .send((second_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((second_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut thirth_templ = first_templ.clone();
        thirth_templ.template_id = 34;

        // Thirth template
        neg_interface
            .send_tp
            .send((second_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut forth_templ = first_templ.clone();
        forth_templ.template_id = 74;

        let mut forth_ph = first_ph.clone();
        forth_ph.template_id = 74;

        // Forth second template and prev hash
        neg_interface
            .send_tp
            .send((forth_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((forth_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        match bridge.safe_lock(|_| {}) {
            Ok(_) => println!("ok"),
            Err(_) => println!("err"),
        };
    }
    #[tokio::test]
    async fn support_multi_downs_jn() {
        let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..32);
        let tx_out_ = vec![
            0_u8, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
            222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
            139, 235, 216, 54, 151, 78, 140, 249,
        ];
        let tx_out_1 = [
            0, 242, 5, 42, 1, 0, 0, 0, 25, 118, 169, 20, 85, 162, 233, 20, 174, 185, 114, 155, 76,
            210, 101, 36, 140, 182, 122, 134, 94, 174, 149, 253, 136, 172,
        ];

        // Create bridge
        let (upstream_kind, neg_interface) = test_utils::create_upstream_kind_neg();
        // interface must not dropped or channels will broks
        let (bridge, _interface) = test_utils::create_bridge(upstream_kind, extranonces);
        let tx_out_1 = roles_logic_sv2::bitcoin::TxOut::consensus_decode(&tx_out_1[..]).unwrap();

        // Send first cb out
        neg_interface
            .send_cb_out
            .send((vec![tx_out_1], vec![0; 4]))
            .await
            .unwrap();

        let first_templ = NewTemplate {
            template_id: 78,
            future_template: true,
            version: 805306368,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![2, 65, 1, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 1250000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: tx_out_.try_into().unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };
        let first_ph = SetNewPrevHashTemplate {
            template_id: 78,
            prev_hash: vec![0; 32].try_into().unwrap(),
            header_timestamp: 98,
            n_bits: 67,
            target: vec![0; 32].try_into().unwrap(),
        };

        // Send first template and prev hash
        neg_interface
            .send_tp
            .send((first_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((first_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Open downstream connection
        bridge
            .safe_lock(|b| b.on_new_sv1_connection(100_000.0))
            .unwrap()
            .unwrap();
        bridge
            .safe_lock(|b| b.on_new_sv1_connection(100_000.0))
            .unwrap()
            .unwrap();
        bridge
            .safe_lock(|b| b.on_new_sv1_connection(100_000.0))
            .unwrap()
            .unwrap();
        bridge
            .safe_lock(|b| b.on_new_sv1_connection(100_000.0))
            .unwrap()
            .unwrap();

        let mut second_templ = first_templ.clone();
        second_templ.template_id = 90;

        let mut second_ph = first_ph.clone();
        second_ph.template_id = 90;

        // Send second template and prev hash
        neg_interface
            .send_tp
            .send((second_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((second_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut thirth_templ = first_templ.clone();
        thirth_templ.template_id = 34;

        // Thirth template
        neg_interface
            .send_tp
            .send((second_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut forth_templ = first_templ.clone();
        forth_templ.template_id = 74;

        let mut forth_ph = first_ph.clone();
        forth_ph.template_id = 74;

        // Forth second template and prev hash
        neg_interface
            .send_tp
            .send((forth_templ.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        neg_interface
            .send_ph
            .send((forth_ph.clone(), vec![0; 4]))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        match bridge.safe_lock(|_| {}) {
            Ok(_) => println!("ok"),
            Err(_) => println!("err"),
        };
    }
}
