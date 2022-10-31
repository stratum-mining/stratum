use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    mining_sv2::{ExtendedExtranonce, NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended},
    utils::{Id, Mutex},
};
use std::sync::Arc;
use v1::{client_to_server::Submit, server_to_client};

use super::next_mining_notify::NextMiningNotify;
use crate::{Error, ProxyResult};
use tracing::warn;

/// Bridge between the SV2 `Upstream` and SV1 `Downstream` responsible for the following messaging
/// translation:
/// 1. SV1 `mining.submit` -> SV2 `SubmitSharesExtended`
/// 2. SV2 `SetNewPrevHash` + `NewExtendedMiningJob` -> SV1 `mining.notify`
#[derive(Debug)]
pub struct Bridge {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    submit_from_sv1: Receiver<(Submit, ExtendedExtranonce)>,
    /// Sends SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages to
    /// the `Upstream`.
    submit_to_sv2: Sender<SubmitSharesExtended<'static>>,
    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` to be translated (along with a
    /// SV2 `NewExtendedMiningJob` message) to a SV1 `mining.submit` for the `Downstream`.
    set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream` to be translated (along
    /// with a SV2 `SetNewPrevHash` message) to a SV1 `mining.submit` to be sent to the
    /// `Downstream`.
    new_extended_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    /// Stores the received SV2 `SetNewPrevHash` and `NewExtendedMiningJob` messages in the
    /// `NextMiningNotify` struct to be translated into a SV1 `mining.notify` message to be sent to
    /// the `Downstream`.
    next_mining_notify: Arc<Mutex<NextMiningNotify>>,
    /// Sends SV1 `mining.notify` message (translated from the SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages stored in the `NextMiningNotify`) to the `Downstream`.
    sender_mining_notify: Sender<server_to_client::Notify>,
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
    last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
}

impl Bridge {
    /// Instantiate a new `Bridge`.
    pub fn new(
        submit_from_sv1: Receiver<(Submit, ExtendedExtranonce)>,
        submit_to_sv2: Sender<SubmitSharesExtended<'static>>,
        set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        new_extended_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        next_mining_notify: Arc<Mutex<NextMiningNotify>>,
        sender_mining_notify: Sender<server_to_client::Notify>,
        last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
    ) -> Self {
        Self {
            submit_from_sv1,
            submit_to_sv2,
            set_new_prev_hash,
            new_extended_mining_job,
            next_mining_notify,
            sender_mining_notify,
            channel_sequence_id: Id::new(),
            last_notify,
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
        let submit_recv = self_.safe_lock(|s| s.submit_from_sv1.clone()).unwrap();
        let submit_to_sv2 = self_.safe_lock(|s| s.submit_to_sv2.clone()).unwrap();
        task::spawn(async move {
            loop {
                let (sv1_submit, extrnonce) = submit_recv.clone().recv().await.unwrap();
                let channel_sequence_id =
                    self_.safe_lock(|s| s.channel_sequence_id.next()).unwrap() - 1;
                let sv2_submit: SubmitSharesExtended =
                    Self::translate_submit(channel_sequence_id, sv1_submit, &extrnonce).unwrap();
                submit_to_sv2.send(sv2_submit).await.unwrap();
            }
        });
    }

    /// Translates a SV1 `mining.submit` message to a SV2 `SubmitSharesExtended` message.
    fn translate_submit(
        channel_sequence_id: u32,
        sv1_submit: Submit,
        extranonce_1: &ExtendedExtranonce,
    ) -> ProxyResult<SubmitSharesExtended<'static>> {
        let extranonce_vec: Vec<u8> = sv1_submit.extra_nonce2.into();
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

    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` and stores it in
    /// `NextMiningNotify` which is translated to a SV1 `mining.notify` message and sent to the
    /// `Downstream` (once a SV2 `NewExtendedMiningJob` message is also received).
    fn handle_new_prev_hash(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                // Receive `SetNewPrevHash` from `Upstream`
                let set_new_prev_hash_recv =
                    self_.safe_lock(|r| r.set_new_prev_hash.clone()).unwrap();
                let sv2_set_new_prev_hash: SetNewPrevHash =
                    set_new_prev_hash_recv.clone().recv().await.unwrap();
                tracing::warn!(
                    "handle_new_prev_hash job_id: {:?}",
                    &sv2_set_new_prev_hash.job_id
                );
                // Store the prevhash value in the `NextMiningNotify` struct
                self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| {
                                nmn.set_new_prev_hash_msg(sv2_set_new_prev_hash);
                                warn!("handle_new_prev_hash nmn set_new_prev_hash: {:?}", &nmn);
                            })
                            .unwrap();
                    })
                    .unwrap();

                // Sender here to Downstream recvier that updates NMN
                // do safe lock to take sender (can do this at begining of loop)
                let sender_mining_notify =
                    self_.safe_lock(|s| s.sender_mining_notify.clone()).unwrap();
                // Create a new `mining.notify` if `NewExtendedMiningJob` has already been
                // received, otherwise gets None
                let sv1_notify_msg = self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| nmn.create_notify())
                            .unwrap()
                    })
                    .unwrap();
                warn!(
                    "handle_new_prev_hash mining.notify to send: {:?}",
                    &sv1_notify_msg
                );

                // If a `mining.notify` was able to be created (aka if `SetNewPrevHash` AND
                // `NewExtendedMiningJob` messages have both been received), send the
                // `mining.notify` data to the `Downstream`
                if let Some(msg) = sv1_notify_msg {
                    warn!("handle_new_prev_hash sending mining.notify to Downstream");
                    // `last_notify` logic here is only relevant for SV2 `SetNewPrevHash` and
                    // `NewExtendedMiningJob` messages received **before** a Downstream role
                    // connects
                    let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                    last_notify
                        .safe_lock(|s| {
                            let _ = s.insert(msg.clone());
                        })
                        .unwrap();
                    sender_mining_notify.send(msg).await.unwrap();
                } else {
                    // info!("Error creating `mining.Notify` from `SetNewPrevHash`");
                    warn!("handle_new_prev_hash NOT sending mining.notify to Downstream");
                }
            }
        });
    }

    /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream` and stores it in
    /// `NextMiningNotify`. Because of the current behavior of Braiins Pool, we do NOT create or
    /// send the `mining.notify` message values to the `Downstream` here. Instead, this is only
    /// done upon receiving a new `SetNewPrevHash` message.
    fn handle_new_extended_mining_job(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                // Receive `NewExtendedMiningJob` from `Upstream`
                let set_new_extended_mining_job_recv = self_
                    .safe_lock(|r| r.new_extended_mining_job.clone())
                    .unwrap();
                let sv2_new_extended_mining_job: NewExtendedMiningJob =
                    set_new_extended_mining_job_recv
                        .clone()
                        .recv()
                        .await
                        .unwrap();
                warn!(
                    "handle_new_extended_mining_job job_id: {:?}",
                    &sv2_new_extended_mining_job.job_id
                );
                // Store the new extended mining job values in the `NextMiningNotify` struct
                self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| {
                                nmn.new_extended_mining_job_msg(sv2_new_extended_mining_job);
                                warn!("handle_new_extended_mining_job nmn set_new_extended_mining_job: {:?}", &nmn);
                            })
                            .unwrap();
                    })
                    .unwrap();

                // Braiins pool currently only sends future_job=true, so there is never a case
                // where we will want to send a mining.notify here because it will not have a
                // corresponding SetNewPrevHash yet.
                let sender_mining_notify =
                    self_.safe_lock(|s| s.sender_mining_notify.clone()).unwrap();

                // If this NewExtendedMiningJob has a future_job=false (meaning that this
                // NewExtendedMiningJob is intended to be used with an already received
                // SetNewPrevHash), a new `mining.notify` will be created via the NextMiningNotify
                // create_notify method and returned in an Option to then be sent to the
                // Downstream. If this NewExtendedMiningJob has a future_job=true (meaning that
                // this NewExtendedMiningJob is intended to be used with a future SetNewPrevHash,
                // the NextMiningNotify create_notify function will return None and NO
                // `mining.notify` will be sent to the Downstream.
                let sv1_notify_msg = self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| nmn.create_notify())
                            .unwrap()
                    })
                    .unwrap();

                // If the current NewExtendedMiningJob and SetNewPrevHash are present and are
                // intended to be used for the same job (both messages job_id's are the same), send
                // the newly created `mining.notify` to the Downstream for mining.
                if let Some(msg) = sv1_notify_msg {
                    warn!("handle_new_extended_mining_job sending mining.notify to Downstream");
                    let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                    last_notify
                        .safe_lock(|s| {
                            let _ = s.insert(msg.clone());
                        })
                        .unwrap();
                    sender_mining_notify.send(msg).await.unwrap();
                } else {
                    warn!("handle_new_extended_mining_job NOT sending mining.notify to Downstream");
                }
            }
        });
    }
}
