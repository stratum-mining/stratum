use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended},
    utils::{Id, Mutex},
};
use std::{collections::HashMap, sync::Arc};
use v1::{client_to_server::Submit, server_to_client};

use super::next_mining_notify::NextMiningNotify;
use crate::{Error, ProxyResult};
use tracing::{debug, error};

/// Bridge between the SV2 `Upstream` and SV1 `Downstream` responsible for the following messaging
/// translation:
/// 1. SV1 `mining.submit` -> SV2 `SubmitSharesExtended`
/// 2. SV2 `SetNewPrevHash` + `NewExtendedMiningJob` -> SV1 `mining.notify`
#[derive(Debug)]
pub struct Bridge {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    rx_sv1_submit: Receiver<(Submit<'static>, ExtendedExtranonce)>,
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
    /// Stores the received SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages in the
    /// `NextMiningNotify` struct to be translated into a SV1 `mining.notify` message to be sent to
    /// the `Downstream`.
    next_mining_notify: Arc<Mutex<NextMiningNotify>>,
    /// Sends SV1 `mining.notify` message (translated from the SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages stored in the `NextMiningNotify`) to the `Downstream`.
    tx_sv1_notify: Sender<server_to_client::Notify<'static>>,
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
    last_notify: Arc<Mutex<Option<server_to_client::Notify<'static>>>>,
    /// Stores SV2 `NewExtendedMiningJob` messages intended for future SV2 `SetNewPrevHash`
    /// messages (when the SV2 `NewExtendedMiningJob` message has a `future_job=false`). The
    /// `job_id` is the key, and the `NewExtendedMiningJob` is the value.
    job_mapper: HashMap<u32, NewExtendedMiningJob<'static>>,
}

impl Bridge {
    /// Instantiate a new `Bridge`.
    pub fn new(
        rx_sv1_submit: Receiver<(Submit<'static>, ExtendedExtranonce)>,
        tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
        rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        next_mining_notify: Arc<Mutex<NextMiningNotify>>,
        tx_sv1_notify: Sender<server_to_client::Notify<'static>>,
        last_notify: Arc<Mutex<Option<server_to_client::Notify<'static>>>>,
    ) -> Self {
        Self {
            rx_sv1_submit,
            tx_sv2_submit_shares_ext,
            rx_sv2_set_new_prev_hash,
            rx_sv2_new_ext_mining_job,
            next_mining_notify,
            tx_sv1_notify,
            channel_sequence_id: Id::new(),
            last_notify,
            job_mapper: HashMap::new(),
        }
    }

    /// Starts the tasks that receive SV1 and SV2 messages to be translated and sent to their
    /// respective roles.
    pub fn start(self) {
        let self_ = Arc::new(Mutex::new(self));
        Self::handle_new_prev_hash(self_.clone());
        Self::handle_new_extended_mining_job(self_.clone());
        Self::handle_downstream_share_submission(self_);
    }

    /// Receives a SV1 `mining.submit` message from the `Downstream`, translates it to a SV2
    /// `SubmitSharesExtended` message, and sends it to the `Upstream`.
    fn handle_downstream_share_submission(self_: Arc<Mutex<Self>>) {
        let rx_sv1_submit = self_.safe_lock(|s| s.rx_sv1_submit.clone()).unwrap();
        let tx_sv2_submit_shares_ext = self_
            .safe_lock(|s| s.tx_sv2_submit_shares_ext.clone())
            .unwrap();
        task::spawn(async move {
            loop {
                let (sv1_submit, extrnonce) = rx_sv1_submit.clone().recv().await.unwrap();
                let channel_sequence_id =
                    self_.safe_lock(|s| s.channel_sequence_id.next()).unwrap() - 1;
                let sv2_submit: SubmitSharesExtended =
                    Self::translate_submit(channel_sequence_id, sv1_submit, &extrnonce).unwrap();
                tx_sv2_submit_shares_ext.send(sv2_submit).await.unwrap();
            }
        });
    }

    /// Translates a SV1 `mining.submit` message to a SV2 `SubmitSharesExtended` message.
    fn translate_submit(
        channel_sequence_id: u32,
        sv1_submit: Submit,
        extranonce_1: &ExtendedExtranonce,
    ) -> ProxyResult<'static, SubmitSharesExtended<'static>> {
        let extranonce_vec: Vec<u8> = sv1_submit.extra_nonce2.0.to_vec();
        let extranonce = extranonce_1
            .without_upstream_part(Some(extranonce_vec.try_into().unwrap()))
            .unwrap();

        let version = match sv1_submit.version_bits {
            Some(vb) => vb.0,
            None => return Err(Error::NoSv1VersionBits),
        };

        Ok(SubmitSharesExtended {
            channel_id: 1,
            sequence_number: channel_sequence_id,
            job_id: sv1_submit.job_id.parse::<u32>()?,
            nonce: sv1_submit.nonce.0,
            ntime: sv1_submit.time.0,
            version,
            extranonce: extranonce.try_into().unwrap(),
        })
    }

    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` and creates a SV1
    /// `mining.notify` message (in conjunction with a previously received SV2
    /// `NewExtendedMiningJob` message) which is sent to the `Downstream`. The protocol requires
    /// that before every received `SetNewPrevHash`, a `NewExtendedMiningJob` with a
    /// corresponding `job_id` has already been received. If this is not the case, an error has
    /// occurred on the Upstream pool role and the connection will close.
    fn handle_new_prev_hash(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                // Receive `SetNewPrevHash` from `Upstream`
                let rx_sv2_set_new_prev_hash = self_
                    .safe_lock(|r| r.rx_sv2_set_new_prev_hash.clone())
                    .unwrap();
                let sv2_set_new_prev_hash: SetNewPrevHash =
                    rx_sv2_set_new_prev_hash.clone().recv().await.unwrap();
                debug!(
                    "handle_new_prev_hash job_id: {:?}",
                    &sv2_set_new_prev_hash.job_id
                );

                // Store the prevhash value in the `NextMiningNotify` struct
                self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| {
                                nmn.set_new_prev_hash_msg(sv2_set_new_prev_hash.clone());
                                debug!("handle_new_prev_hash nmn set_new_prev_hash: {:?}", &nmn);
                            })
                            .unwrap();
                    })
                    .unwrap();

                // Check the job_mapper to see if there is a NewExtendedMiningJob with a job id
                // that matches this SetNewPrevHash's job id. If there is, remove it from the
                // mapping and store this NewExtendedMiningJob in the NextMiningNotify for
                // mining.notify creation. If there is not, the NewExtendedMiningJob that is
                // already stored in the NextMiningNotify struct will be favored.
                self_
                    .safe_lock(|s| {
                        if let Some(job) = s.job_mapper.remove(&sv2_set_new_prev_hash.job_id) {
                            s.next_mining_notify
                                .safe_lock(|nmn| {
                                    nmn.new_extended_mining_job_msg(job);
                                })
                                .unwrap();
                        }
                    })
                    .unwrap();

                // Get the sender to send the mining.notify to the Downstream
                let tx_sv1_notify = self_.safe_lock(|s| s.tx_sv1_notify.clone()).unwrap();

                // Create the mining.notify to be sent to the Downstream. The create_notify method
                // checks that this NewExtendedMiningJob and the previously stored SetNewPrevHash
                // have matching job ids. If they are not matching, an error has occurred on the
                // Upstream pool role. In this case, create_notify will return None and the
                // connection will close.
                let sv1_notify_msg = self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| nmn.create_notify())
                            .unwrap()
                    })
                    .unwrap();
                debug!(
                    "handle_new_prev_hash mining.notify to send: {:?}",
                    &sv1_notify_msg
                );

                // If the current NewExtendedMiningJob and SetNewPrevHash are present and are
                // intended to be used for the same job (both messages job_id's are the same), send
                // the newly created `mining.notify` to the Downstream for mining. Otherwise, an
                // error has occurred on the Upstream pool role and the connection will close.
                if let Some(msg) = sv1_notify_msg {
                    debug!(
                        "handle_new_prev_hash sending mining.notify to Downstream: {:?}",
                        &msg
                    );
                    // `last_notify` logic here is only relevant for SV2 `SetNewPrevHash` and
                    // `NewExtendedMiningJob` messages received **before** a Downstream role
                    // connects
                    let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                    last_notify
                        .safe_lock(|s| {
                            let _ = s.insert(msg.clone());
                        })
                        .unwrap();
                    tx_sv1_notify.send(msg).await.unwrap();

                    // Flush stale jobs from job_mapper (aka retain all values greater than
                    // this job_id)
                    let last_stale_job_id = sv2_set_new_prev_hash.job_id;
                    self_
                        .safe_lock(|s| {
                            s.job_mapper.retain(|&k, _| k > last_stale_job_id);
                        })
                        .unwrap();
                } else {
                    error!("NewExtendedMiningJob and SetNewPrevHash job ids mismatch");
                    panic!("NewExtendedMiningJob and SetNewPrevHash job ids mismatch");
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
        task::spawn(async move {
            loop {
                // Receive `NewExtendedMiningJob` from `Upstream`
                let rx_sv2_new_ext_mining_job = self_
                    .safe_lock(|r| r.rx_sv2_new_ext_mining_job.clone())
                    .unwrap();
                let sv2_new_extended_mining_job: NewExtendedMiningJob =
                    rx_sv2_new_ext_mining_job.clone().recv().await.unwrap();
                debug!(
                    "handle_new_extended_mining_job job_id: {:?}",
                    &sv2_new_extended_mining_job.job_id
                );

                // If future_job=true, this job is meant for a future SetNewPrevHash that the proxy
                // has yet to receive. Insert this new job into the job_mapper (will overwrite any
                // previous jobs with the same job id).
                // If future_job=false, this job is meant for the current SetNewPrevHash. The
                // NextMiningNotify struct is updated with this job, and (after double checking
                // this job's job_id matches the SetNewPrevHash job id stored in NextMiningNotify)
                // a mining.notify is created and sent to the Downstream. If these job ids do not
                // match, an error has occured on the Upstream pool role and the connection will
                // close.
                if sv2_new_extended_mining_job.future_job {
                    self_
                        .safe_lock(|s| {
                            // Insert new future job, replaces value if already exists
                            let _ = s.job_mapper.insert(
                                sv2_new_extended_mining_job.job_id,
                                sv2_new_extended_mining_job.clone(),
                            );
                        })
                        .unwrap();
                } else {
                    // Get the sender to send the mining.notify to the Downstream
                    let tx_sv1_notify = self_.safe_lock(|s| s.tx_sv1_notify.clone()).unwrap();

                    // Insert the new job into the NextMiningNotify struct and create the
                    // mining.notify to be sent to the Downstream. The create_notify method checks
                    // that this NewExtendedMiningJob and the previously stored SetNewPrevHash have
                    // matching job ids. If they are not matching, an error has occurred on the
                    // Upstream pool role. In this case, create_notify will return None and the
                    // connection will close.
                    let sv1_notify_msg = self_
                        .safe_lock(|s| {
                            s.next_mining_notify
                                .safe_lock(|nmn| {
                                    nmn.new_extended_mining_job_msg(
                                        sv2_new_extended_mining_job.clone(),
                                    );
                                    nmn.create_notify()
                                })
                                .unwrap()
                        })
                        .unwrap();

                    // If the current NewExtendedMiningJob and SetNewPrevHash are present and are
                    // intended to be used for the same job (both messages job_id's are the same), send
                    // the newly created `mining.notify` to the Downstream for mining.
                    if let Some(msg) = sv1_notify_msg {
                        debug!(
                            "handle_new_extended_mining_job sending mining.notify to Downstream"
                        );
                        // TODO: handle the last_notify using the job_mapper instead
                        let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                        last_notify
                            .safe_lock(|s| {
                                let _ = s.insert(msg.clone());
                            })
                            .unwrap();
                        tx_sv1_notify.send(msg).await.unwrap();

                        // Flush stale jobs from job_mapper (aka retain all values greater than
                        // this job_id)
                        let last_stale_job_id = sv2_new_extended_mining_job.job_id;
                        self_
                            .safe_lock(|s| {
                                s.job_mapper.retain(|&k, _| k > last_stale_job_id);
                            })
                            .unwrap();
                    } else {
                        error!("NewExtendedMiningJob and SetNewPrevHash job ids mismatch");
                        panic!("NewExtendedMiningJob and SetNewPrevHash job ids mismatch");
                    }
                }
            }
        });
    }
}
