use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    channel_logic::channel_factory::{ExtendedChannelKind, ProxyExtendedChannelFactory},
    mining_sv2::{
        ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended, Target,
    },
    parsers::Mining,
    utils::{GroupId, Id, Mutex},
};
use std::sync::Arc;
use tokio::sync::broadcast;
use v1::{client_to_server::Submit, server_to_client};

use crate::{
    error::Error::{self, PoisonLock},
    status, ProxyResult,
};
use error_handling::handle_result;
use roles_logic_sv2::{channel_logic::channel_factory::OnNewShare, Error as RolesLogicError};
use tracing::{debug, error, info};

/// Bridge between the SV2 `Upstream` and SV1 `Downstream` responsible for the following messaging
/// translation:
/// 1. SV1 `mining.submit` -> SV2 `SubmitSharesExtended`
/// 2. SV2 `SetNewPrevHash` + `NewExtendedMiningJob` -> SV1 `mining.notify`
#[derive(Debug)]
pub struct Bridge {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    rx_sv1_submit: Receiver<(Submit<'static>, Vec<u8>)>,
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
    /// Unique sequential identifier of the submit within the channel.
    channel_sequence_id: Id,
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
}

impl Bridge {
    #[allow(clippy::too_many_arguments)]
    /// Instantiate a new `Bridge`.
    pub fn new(
        rx_sv1_submit: Receiver<(Submit<'static>, Vec<u8>)>,
        tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
        rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        extranonces: ExtendedExtranonce,
        target: Arc<Mutex<Vec<u8>>>,
        up_id: u32,
    ) -> Self {
        let ids = Arc::new(Mutex::new(GroupId::new()));
        let share_per_min = 1.0;
        let upstream_target: [u8; 32] =
            target.safe_lock(|t| t.clone()).unwrap().try_into().unwrap();
        let upstream_target: Target = upstream_target.into();
        let kind = ExtendedChannelKind::Proxy { upstream_target };
        Self {
            rx_sv1_submit,
            tx_sv2_submit_shares_ext,
            rx_sv2_set_new_prev_hash,
            rx_sv2_new_ext_mining_job,
            tx_sv1_notify,
            tx_status,
            channel_sequence_id: Id::new(),
            last_notify: None,
            channel_factory: ProxyExtendedChannelFactory::new(
                ids,
                extranonces,
                None,
                share_per_min,
                kind,
                None,
                up_id,
            ),
            future_jobs: vec![],
            last_p_hash: None,
            target,
        }
    }

    pub fn on_new_sv1_connection(&mut self, hash_rate: f32) -> Option<OpenSv1Downstream> {
        match self.channel_factory.new_extended_channel(0, hash_rate, 0) {
            Some(messages) => {
                for message in messages {
                    match message {
                        Mining::OpenExtendedMiningChannelSuccess(success) => {
                            let extranonce = success.extranonce_prefix.to_vec();
                            let extranonce2_len = success.extranonce_size;
                            let target = success.target.to_vec();
                            return Some(OpenSv1Downstream {
                                last_notify: self.last_notify.clone(),
                                extranonce,
                                target,
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
            None => todo!(),
        };
        None
    }

    /// Starts the tasks that receive SV1 and SV2 messages to be translated and sent to their
    /// respective roles.
    pub fn start(self_: Arc<Mutex<Self>>) {
        Self::handle_new_prev_hash(self_.clone());
        Self::handle_new_extended_mining_job(self_.clone());
        Self::handle_downstream_share_submission(self_);
    }

    /// Receives a SV1 `mining.submit` message from the `Downstream`, translates it to a SV2
    /// `SubmitSharesExtended` message, and sends it to the `Upstream`.
    fn handle_downstream_share_submission(self_: Arc<Mutex<Self>>) {
        let (rx_sv1_submit, tx_sv2_submit_shares_ext, target_mutex, tx_status) = self_
            .safe_lock(|s| {
                (
                    s.rx_sv1_submit.clone(),
                    s.tx_sv2_submit_shares_ext.clone(),
                    s.target.clone(),
                    s.tx_status.clone(),
                )
            })
            .unwrap();
        task::spawn(async move {
            loop {
                let target = target_mutex
                    .safe_lock(|t| t.clone())
                    .map_err(|_| PoisonLock);
                let target = handle_result!(tx_status, target).try_into();
                let upstream_target: [u8; 32] = handle_result!(tx_status, target);
                let mut upstream_target: Target = upstream_target.into();
                let res = self_
                    .safe_lock(|s| s.channel_factory.set_target(&mut upstream_target))
                    .map_err(|_| PoisonLock);
                handle_result!(tx_status, res);

                let (sv1_submit, extrnonce) =
                    handle_result!(tx_status, rx_sv1_submit.clone().recv().await);
                let channel_sequence_id = self_
                    .safe_lock(|s| s.channel_sequence_id.next())
                    .map_err(|_| PoisonLock);
                let channel_sequence_id = handle_result!(tx_status, channel_sequence_id) - 1;
                let sv2_submit = self_
                    .safe_lock(|s| s.translate_submit(channel_sequence_id, sv1_submit, extrnonce))
                    .map_err(|_| PoisonLock);
                let sv2_submit = handle_result!(tx_status, handle_result!(tx_status, sv2_submit));
                let mut send_upstream = false;
                let res = self_
                    .safe_lock(|s| {
                        s.channel_factory.on_submit_shares_extended(
                            sv2_submit.clone(),
                            Some(crate::SELF_EXTRNONCE_LEN),
                        )
                    })
                    .map_err(|_| PoisonLock);

                match res {
                    Ok(Ok(OnNewShare::SendErrorDownstream(e))) => {
                        error!(
                            "Submit share error {:?}",
                            std::str::from_utf8(&e.error_code.to_vec()[..])
                        );
                    }
                    Ok(Ok(OnNewShare::SendSubmitShareUpstream(_))) => {
                        info!("SHARE MEETS TARGET");
                        send_upstream = true;
                    }
                    Ok(Ok(OnNewShare::RelaySubmitShareUpstream)) => {
                        info!("SHARE MEETS TARGET");
                        send_upstream = true;
                    }
                    Ok(Ok(OnNewShare::ShareMeetBitcoinTarget(_))) => unreachable!(),
                    Ok(Ok(OnNewShare::ShareMeetDownstreamTarget)) => {
                        info!("SHARE MEETS DOWNSTREAM TARGET")
                    }
                    Ok(Err(e)) => error!("Error: {:?}", e),
                    Err(e) => handle_result!(tx_status, Err(e)),
                }
                if send_upstream {
                    handle_result!(tx_status, tx_sv2_submit_shares_ext.send(sv2_submit).await)
                };
            }
        });
    }

    /// Translates a SV1 `mining.submit` message to a SV2 `SubmitSharesExtended` message.
    fn translate_submit(
        &self,
        channel_sequence_id: u32,
        sv1_submit: Submit,
        extranonce: Vec<u8>,
    ) -> ProxyResult<'static, SubmitSharesExtended<'static>> {
        let version = match sv1_submit.version_bits {
            Some(vb) => vb.0,
            None => self
                .channel_factory
                .last_valid_job_version()
                .ok_or(Error::RolesSv2Logic(RolesLogicError::NoValidJob))?,
        };

        Ok(SubmitSharesExtended {
            channel_id: 1,
            sequence_number: channel_sequence_id,
            job_id: sv1_submit.job_id.parse::<u32>()?,
            nonce: sv1_submit.nonce.0,
            ntime: sv1_submit.time.0,
            version,
            extranonce: extranonce.try_into()?,
        })
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
                let res = self_
                    .safe_lock(|s| s.last_p_hash = Some(sv2_set_new_prev_hash.clone()))
                    .map_err(|_| PoisonLock);
                handle_result!(tx_status, res);
                let on_new_prev_hash_res = self_
                    .safe_lock(|s| {
                        s.channel_factory
                            .on_new_prev_hash(sv2_set_new_prev_hash.clone())
                    })
                    .map_err(|_| PoisonLock);
                let on_new_prev_hash_res = handle_result!(tx_status, on_new_prev_hash_res);
                handle_result!(tx_status, on_new_prev_hash_res);

                let future_jobs = self_
                    .safe_lock(|s| {
                        let future_jobs = s.future_jobs.clone();
                        s.future_jobs = vec![];
                        future_jobs
                    })
                    .map_err(|_| PoisonLock);
                let mut future_jobs = handle_result!(tx_status, future_jobs);

                while let Some(job) = future_jobs.pop() {
                    if job.job_id == sv2_set_new_prev_hash.job_id {
                        // Create the mining.notify to be sent to the Downstream.
                        let notify = crate::proxy::next_mining_notify::create_notify(
                            sv2_set_new_prev_hash.clone(),
                            job,
                        );
                        // Get the sender to send the mining.notify to the Downstream
                        handle_result!(tx_status, tx_sv1_notify.send(notify.clone()));
                        let res = self_
                            .safe_lock(|s| {
                                s.last_notify = Some(notify);
                            })
                            .map_err(|_| PoisonLock);
                        handle_result!(tx_status, res);
                        break;
                    }
                }
            }
        });
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
                let sv2_new_extended_mining_job: NewExtendedMiningJob =
                    handle_result!(tx_status, rx_sv2_new_ext_mining_job.clone().recv().await);
                debug!(
                    "handle_new_extended_mining_job job_id: {:?}",
                    &sv2_new_extended_mining_job.job_id
                );
                let res = self_
                    .safe_lock(|s| {
                        s.channel_factory.on_new_extended_mining_job(
                            sv2_new_extended_mining_job.as_static().clone(),
                        )
                    })
                    .map_err(|_| PoisonLock);

                handle_result!(tx_status, handle_result!(tx_status, res));

                // If future_job=true, this job is meant for a future SetNewPrevHash that the proxy
                // has yet to receive. Insert this new job into the job_mapper .
                if sv2_new_extended_mining_job.future_job {
                    let res = self_
                        .safe_lock(|s| s.future_jobs.push(sv2_new_extended_mining_job.clone()))
                        .map_err(|_| PoisonLock);
                    handle_result!(tx_status, res);

                // If future_job=false, this job is meant for the current SetNewPrevHash.
                } else {
                    let last_p_hash_res = self_
                        .safe_lock(|s| s.last_p_hash.clone())
                        .map_err(|_| PoisonLock);
                    let last_p_hash_option = handle_result!(tx_status, last_p_hash_res);
                    // last_p_hash is an Option<SetNewPrevHash> so we need to map to the correct error type to be handled
                    let last_p_hash = handle_result!(
                        tx_status,
                        last_p_hash_option.ok_or(Error::RolesSv2Logic(
                            RolesLogicError::JobIsNotFutureButPrevHashNotPresent
                        ))
                    );
                    // Create the mining.notify to be sent to the Downstream.
                    let notify = crate::proxy::next_mining_notify::create_notify(
                        last_p_hash,
                        sv2_new_extended_mining_job,
                    );
                    // Get the sender to send the mining.notify to the Downstream
                    handle_result!(tx_status, tx_sv1_notify.send(notify.clone()));
                    let res = self_
                        .safe_lock(|s| {
                            s.last_notify = Some(notify);
                        })
                        .map_err(|_| PoisonLock);
                    handle_result!(tx_status, res);
                }
            }
        });
    }
}
pub struct OpenSv1Downstream {
    pub last_notify: Option<server_to_client::Notify<'static>>,
    pub extranonce: Vec<u8>,
    pub target: Vec<u8>,
    pub extranonce2_len: u16,
}

#[cfg(test)]
mod test {
    use super::*;
    use async_channel::bounded;
    use roles_logic_sv2::bitcoin::util::psbt::serialize::Serialize;
    const EXTRANONCE_LEN: usize = 16;
    pub mod test_utils {
        use super::*;
        pub fn create_bridge() -> Bridge {
            let (_tx_sv1_submit, rx_sv1_submit) = bounded(1);
            let (tx_sv2_submit_shares_ext, _rx_sv2_submit_shares_ext) = bounded(1);
            let (_tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(1);
            let (_tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(1);
            let (tx_sv1_notify, _rx_sv1_notify) = broadcast::channel(1);
            let (tx_status, _rx_status) = bounded(1);
            let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..EXTRANONCE_LEN);
            let upstream_target = vec![
                0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0,
            ];

            Bridge::new(
                rx_sv1_submit,
                tx_sv2_submit_shares_ext,
                rx_sv2_set_new_prev_hash,
                rx_sv2_new_ext_mining_job,
                tx_sv1_notify,
                status::Sender::Bridge(tx_status),
                extranonces,
                Arc::new(Mutex::new(upstream_target)),
                1,
            )
        }

        pub fn create_sv1_submit(job_id: u32) -> Submit<'static> {
            Submit {
                user_name: "test_user".to_string(),
                job_id: job_id.to_string(),
                extra_nonce2: v1::utils::Extranonce::try_from([0; 32].to_vec()).unwrap(),
                time: v1::utils::HexU32Be(1),
                nonce: v1::utils::HexU32Be(1),
                version_bits: None,
                id: "test_id".to_string(),
            }
        }
    }

    #[test]
    fn test_version_bits_insert() {
        use roles_logic_sv2::bitcoin::hashes::Hash;
        let mut bridge = test_utils::create_bridge();
        let out_id = roles_logic_sv2::bitcoin::hashes::sha256d::Hash::from_slice(&[
            0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ])
        .unwrap();
        let p_out = roles_logic_sv2::bitcoin::OutPoint {
            txid: roles_logic_sv2::bitcoin::Txid::from_hash(out_id),
            vout: 0xffff_ffff,
        };
        let in_ = roles_logic_sv2::bitcoin::TxIn {
            previous_output: p_out,
            script_sig: vec![89_u8; EXTRANONCE_LEN].into(),
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
            channel_id: 1,
            job_id: 0,
            prev_hash: [
                3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
                3, 3, 3, 3,
            ]
            .into(),
            min_ntime: 989898,
            nbits: 9,
        };
        bridge.channel_factory.on_new_prev_hash(prev_hash).unwrap();
        let new_mining_job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 0,
            future_job: false,
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
        let channel_seq_id = bridge.channel_sequence_id.next() - 1;
        let sv2_message = bridge
            .translate_submit(channel_seq_id, sv1_submit, vec![0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap();
        // assert sv2 message equals sv1 with version bits added
        assert_eq!(
            new_mining_job.version, sv2_message.version,
            "Version bits were not inserted for non version rolling sv1 message"
        );
    }
}
