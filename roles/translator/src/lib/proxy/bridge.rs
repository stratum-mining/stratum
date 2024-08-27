use async_channel::{Receiver, Sender};
use roles_logic_sv2::{
    channel_logic::channel_factory::{ExtendedChannelKind, ProxyExtendedChannelFactory, Share},
    mining_sv2::{
        ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended, Target,
    },
    parsers::Mining,
    utils::{GroupId, Mutex},
};
use std::sync::Arc;
use tokio::{sync::broadcast, task::AbortHandle};
use v1::{client_to_server::Submit, server_to_client, utils::HexU32Be};

use super::super::{
    downstream_sv1::{DownstreamMessages, SetDownstreamTarget, SubmitShareWithChannelId},
    error::{
        Error::{self, PoisonLock},
        ProxyResult,
    },
    status,
};
use error_handling::handle_result;
use roles_logic_sv2::{channel_logic::channel_factory::OnNewShare, Error as RolesLogicError};
use tracing::{debug, error, info, warn};

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
    last_job_id: u32,
    task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
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
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) -> Arc<Mutex<Self>> {
        let ids = Arc::new(Mutex::new(GroupId::new()));
        let share_per_min = 1.0;
        let upstream_target: [u8; 32] =
            target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
        let upstream_target: Target = upstream_target.into();
        Arc::new(Mutex::new(Self {
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
                None,
                share_per_min,
                ExtendedChannelKind::Proxy { upstream_target },
                None,
                String::from(""),
                up_id,
            ),
            future_jobs: vec![],
            last_p_hash: None,
            target,
            last_job_id: 0,
            task_collector,
        }))
    }

    #[allow(clippy::result_large_err)]
    pub fn on_new_sv1_connection(
        &mut self,
        hash_rate: f32,
    ) -> ProxyResult<'static, OpenSv1Downstream> {
        match self.channel_factory.new_extended_channel(0, hash_rate, 0) {
            Ok(messages) => {
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
            Err(_) => {
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
        let task_collector_handle_downstream =
            self_.safe_lock(|b| b.task_collector.clone()).unwrap();
        let (rx_sv1_downstream, tx_status) = self_
            .safe_lock(|s| (s.rx_sv1_downstream.clone(), s.tx_status.clone()))
            .unwrap();
        let handle_downstream = tokio::task::spawn(async move {
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
        let _ = task_collector_handle_downstream.safe_lock(|a| {
            a.push((
                handle_downstream.abort_handle(),
                "handle_downstream_message".to_string(),
            ))
        });
    }
    /// receives a `SetDownstreamTarget` and updates the downstream target for the channel
    #[allow(clippy::result_large_err)]
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
            .safe_lock(|s| s.channel_factory.on_submit_shares_extended(sv2_submit))
            .map_err(|_| PoisonLock);

        match res {
            Ok(Ok(OnNewShare::SendErrorDownstream(e))) => {
                warn!(
                    "Submit share error {:?}",
                    std::str::from_utf8(&e.error_code.to_vec()[..])
                );
            }
            Ok(Ok(OnNewShare::SendSubmitShareUpstream((share, _)))) => {
                info!("SHARE MEETS UPSTREAM TARGET");
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
                debug!("SHARE MEETS DOWNSTREAM TARGET");
            }
            // Proxy do not have JD capabilities
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
    #[allow(clippy::result_large_err)]
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
                    true,
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
        let task_collector_handle_new_prev_hash =
            self_.safe_lock(|b| b.task_collector.clone()).unwrap();
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
        let handle_new_prev_hash = tokio::task::spawn(async move {
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
        let _ = task_collector_handle_new_prev_hash.safe_lock(|a| {
            a.push((
                handle_new_prev_hash.abort_handle(),
                "handle_new_prev_hash".to_string(),
            ))
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
            // clean_jobs must be false because it's not a NewPrevHash template
            let notify = crate::proxy::next_mining_notify::create_notify(
                last_p_hash,
                sv2_new_extended_mining_job.clone(),
                false,
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
        let task_collector_new_extended_mining_job =
            self_.safe_lock(|b| b.task_collector.clone()).unwrap();
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
        let handle_new_extended_mining_job = tokio::task::spawn(async move {
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
        let _ = task_collector_new_extended_mining_job.safe_lock(|a| {
            a.push((
                handle_new_extended_mining_job.abort_handle(),
                "handle_new_extended_mining_job".to_string(),
            ))
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
    use async_channel::bounded;

    use stratum_common::bitcoin::util::psbt::serialize::Serialize;

    pub mod test_utils {
        use super::*;

        pub struct BridgeInterface {
            pub tx_sv1_submit: Sender<DownstreamMessages>,
            pub rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
            pub tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
            pub tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
            pub rx_sv1_notify: broadcast::Receiver<server_to_client::Notify<'static>>,
        }

        pub fn create_bridge(
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

            let task_collector = Arc::new(Mutex::new(vec![]));
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
                task_collector,
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
        use stratum_common::{
            bitcoin,
            bitcoin::{blockdata::witness::Witness, hashes::Hash},
        };

        let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..16);
        let (bridge, _) = test_utils::create_bridge(extranonces);
        bridge
            .safe_lock(|bridge| {
                let channel_id = 1;
                let out_id = bitcoin::hashes::sha256d::Hash::from_slice(&[
                    0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0,
                ])
                .unwrap();
                let p_out = bitcoin::OutPoint {
                    txid: bitcoin::Txid::from_hash(out_id),
                    vout: 0xffff_ffff,
                };
                let in_ = bitcoin::TxIn {
                    previous_output: p_out,
                    script_sig: vec![89_u8; 16].into(),
                    sequence: bitcoin::Sequence(0),
                    witness: Witness::from_vec(vec![]),
                };
                let tx = bitcoin::Transaction {
                    version: 1,
                    lock_time: bitcoin::PackedLockTime(0),
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
}
