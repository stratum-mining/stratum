///
/// Bridge is a Proxy server that sits between a Downstream role (most typically a SV1 Mining
/// Device, but could also be a SV1 Proxy server) and an Upstream role (most typically a SV2 Pool
/// server, but could also be a SV2 Proxy server). It accepts and sends messages between the SV1
/// Downstream role and the SV2 Upstream role, translating the messages into the appropriate
/// protocol.
///
/// **Bridge starts**
///
/// 1. Connects to SV2 Upstream role.
///    a. Sends a SV2 `SetupConnection` message to the SV2 Upstream role + receives a SV2
///       `SetupConnectionSuccess` or `SetupConnectionError` message in response.
///    b.  SV2 Upstream role immediately sends a SV2 `SetNewPrevHash` + `NewExtendedMiningJob`
///        message.
///    c. If connection was successful, sends a SV2 `OpenExtendedMiningChannel` message to the SV2
///       Upstream role + receives a SV2 `OpenExtendedMiningChannelSuccess` or
///       `OpenMiningChannelError` message in response.
///
/// 2. Meanwhile, Bridge is listening for a SV1 Downstream role to connect. On connection:
///    a. Receives a SV1 `mining.subscribe` message from the SV1 Downstream role + sends a response
///       with a SV1 `mining.set_difficulty` + `mining.notify` which the Bridge builds using
///       the SV2 `SetNewPrevHash` + `NewExtendedMiningJob` messages received from the SV2 Upstream
///       role.
///
/// 3. Bridge waits for the SV1 Downstream role to find a valid share submission.
///    a. It receives this share submission via a SV1 `mining.submit` message + translates it into a
///       SV2 `SubmitSharesExtended` message which is then sent to the SV2 Upstream role + receives
///       a SV2 `SubmitSharesSuccess` or `SubmitSharesError` message in response.
///    b. This keeps happening until a new Bitcoin block is confirmed on the network, making this
///       current job's previous hash stale.
///
/// 4. When a new block is confirmed on the Bitcoin network, the Bridge sends a fresh job to the
///    SV1 Downstream role.
///    a. The SV2 Upstream role immediately sends the Bridge a fresh SV2 `SetNewPrevHash`
///       followed by a `NewExtendedMiningJob` message.
///    b. Once the Bridge receives BOTH messages, it translates them into a SV1 `mining.notify`
///       message + sends to the SV1 Downstream role.
///    c. The SV1 Downstream role begins finding a new valid share submission + Step 3 commences
///       again.
///
use async_channel::{Receiver, Sender};
use async_std::task;
use roles_logic_sv2::{
    mining_sv2::{NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended},
    utils::{Id, Mutex},
};
use std::sync::Arc;
use v1::{client_to_server::Submit, server_to_client};

use super::next_mining_notify::NextMiningNotify;
use crate::{Error, ProxyResult};

#[derive(Debug)]
pub struct Bridge {
    /// Receives a `mining.submit` SV1 message from the SV1 Downstream role.
    submit_from_sv1: Receiver<(Submit, Vec<u8>)>,
    /// Sends `SubmitSharesExtended` SV2 message created on a valid SV1 `mining.submit` message to
    /// the SV2 Upstream.
    submit_to_sv2: Sender<SubmitSharesExtended<'static>>,
    /// `SetNewPrevHash` SV2 message received from the SV2 Upstream.
    set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    /// `NexExtendedMiningJob` SV2 message received from the SV2 Upstream.
    new_extended_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    next_mining_notify: Arc<Mutex<NextMiningNotify>>,
    // TODO: put sender her eor in Bridge to update Dowstream
    sender_mining_notify: Sender<server_to_client::Notify>,
    channel_sequence_id: Id,
    last_notify: Arc<Mutex<Option<server_to_client::Notify>>>,
}

impl Bridge {
    /// Creates a new `Bridge`.
    pub fn new(
        submit_from_sv1: Receiver<(Submit, Vec<u8>)>,
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

    pub fn start(self) {
        let self_ = Arc::new(Mutex::new(self));
        Self::handle_new_prev_hash(self_.clone());
        Self::handle_new_extended_mining_job(self_.clone());
        Self::handle_downstream_share_submission(self_);
    }

    fn handle_downstream_share_submission(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let submit_recv = self_.safe_lock(|s| s.submit_from_sv1.clone()).unwrap();
                let (sv1_submit, extrnonce_1) = submit_recv.clone().recv().await.unwrap();
                let channel_sequence_id =
                    self_.safe_lock(|s| s.channel_sequence_id.next()).unwrap() - 1;
                let sv2_submit: SubmitSharesExtended =
                    Self::translate_submit(channel_sequence_id, sv1_submit, extrnonce_1).unwrap();
                let submit_to_sv2 = self_.safe_lock(|s| s.submit_to_sv2.clone()).unwrap();
                submit_to_sv2.send(sv2_submit).await.unwrap();
            }
        });
    }

    fn translate_submit(
        channel_sequence_id: u32,
        sv1_submit: Submit,
        mut extranonce_1: Vec<u8>,
    ) -> ProxyResult<SubmitSharesExtended<'static>> {
        let mut extranonce_vec: Vec<u8> = sv1_submit.extra_nonce2.into();
        extranonce_1.append(&mut extranonce_vec);
        let extranonce_vec = extranonce_1;
        let extranonce: binary_sv2::B032 = extranonce_vec.try_into()?;

        let version = match sv1_submit.version_bits {
            Some(vb) => vb.0,
            None => return Err(Error::NoSv1VersionBits),
        };
        //TODO
        //let version = 536870916;

        Ok(SubmitSharesExtended {
            channel_id: 1,
            sequence_number: channel_sequence_id,
            job_id: sv1_submit.job_id.parse::<u32>()?,
            nonce: sv1_submit.nonce.0,
            ntime: sv1_submit.time.0,
            version,
            extranonce,
        })
    }

    fn handle_new_prev_hash(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let set_new_prev_hash_recv =
                    self_.safe_lock(|r| r.set_new_prev_hash.clone()).unwrap();
                let sv2_set_new_prev_hash: SetNewPrevHash =
                    set_new_prev_hash_recv.clone().recv().await.unwrap();
                self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| {
                                nmn.set_new_prev_hash_msg(sv2_set_new_prev_hash);
                            })
                            .unwrap();
                    })
                    .unwrap();
                // Sender here to Downstream recvier that updates NMN
                // do safe lock to take sender (can do this at begining of loop)
                let sender_mining_notify =
                    self_.safe_lock(|s| s.sender_mining_notify.clone()).unwrap();
                let sv1_notify_msg = self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| nmn.create_notify())
                            .unwrap()
                    })
                    .unwrap();
                let sv1_notify_msg =
                    sv1_notify_msg.expect("Error creating `mining.Notify` from `SetNewPrevHash`");
                //TODO
                if let Some(msg) = Some(sv1_notify_msg) {
                    let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                    last_notify
                        .safe_lock(|s| {
                            let _ = s.insert(msg.clone());
                        })
                        .unwrap();
                    sender_mining_notify.send(msg).await.unwrap();
                }
            }
        });
    }

    fn handle_new_extended_mining_job(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let set_new_extended_mining_job_recv = self_
                    .safe_lock(|r| r.new_extended_mining_job.clone())
                    .unwrap();
                let sv2_new_extended_mining_job: NewExtendedMiningJob =
                    set_new_extended_mining_job_recv
                        .clone()
                        .recv()
                        .await
                        .unwrap();
                self_
                    .safe_lock(|s| {
                        s.next_mining_notify
                            .safe_lock(|nmn| {
                                nmn.new_extended_mining_job_msg(sv2_new_extended_mining_job);
                            })
                            .unwrap();
                    })
                    .unwrap();
                //let sender_mining_notify =
                //    self_.safe_lock(|s| s.sender_mining_notify.clone()).unwrap();
                //let sv1_notify_msg = self_
                //    .safe_lock(|s| {
                //        s.next_mining_notify
                //            .safe_lock(|nmn| nmn.create_notify())
                //            .unwrap()
                //    })
                //    .unwrap();
                //let sv1_notify_msg = sv1_notify_msg
                //    .expect("Error creating `mining.Notify` from `NewExtendedMiningJob`");
                //if let Some(msg) = sv1_notify_msg {
                //    let last_notify = self_.safe_lock(|s| s.last_notify.clone()).unwrap();
                //    last_notify.safe_lock(|s| {let _ = s.insert(msg.clone());}).unwrap();
                //    sender_mining_notify.send(msg).await.unwrap();
                //}
            }
        });
    }
}
