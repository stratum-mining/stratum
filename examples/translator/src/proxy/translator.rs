///
/// Bridge is a Proxy server sits between a Downstream role (most typically a SV1 Mining
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
///       current job's PrevHash stale.
///
/// 4. When a new block is confirmed on the Bitcoin network, the Bridge sends a fresh job to
///    the SV1 Downstream role.
///    a. The SV2 Upstream role immediately sends the Bridge a fresh SV2 `SetNewPrevHash`
///       followed by a `NewExtendedMiningJob` message.
///    b. Once the Bridge receives BOTH messages, it translates them into a SV1 `mining.notify`
///       message + sends to the SV1 Downstream role.
///    c. The SV1 Downstream role begins finding a new valid share submission + Step 3 commences
///       again.
///
use v1::client_to_server::Submit;
use roles_logic_sv2::mining_sv2::{SetNewPrevHash,SubmitSharesExtended,NewExtendedMiningJob};
use roles_logic_sv2::utils::Mutex;
use async_std::task;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Bridge {
    submit_from_sv1: Receiver<Submit>,
    submit_to_sv2: Sender<SubmitSharesExtended<'static>>,

    set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,

    new_extended_mining_job: Receiver<NewExtendedMiningJob<'static>>,
}

use async_channel::{Receiver, Sender};


impl Bridge {
    pub fn new(
        submit_from_sv1: Receiver<Submit>,
        submit_to_sv2: Sender<SubmitSharesExtended<'static>>,

        set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,

        new_extended_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        ) -> Self
    {
        Self {
            submit_from_sv1,
            submit_to_sv2,
            set_new_prev_hash,
            new_extended_mining_job,
        }
    }

    pub fn start(self) {
        let self_ = Arc::new(Mutex::new(self));
        Self::handle_new_prev_hash(self_.clone());
        Self::handle_new_extended_minig_job(self_.clone());
        Self::handle_downstream_share_submission(self_.clone());
    }

    fn handle_downstream_share_submission(self_: Arc<Mutex<Self>>) {
        task::spawn(async move {
            loop {
                let submit_recv = self_.safe_lock(|s| s.submit_from_sv1.clone()).unwrap();
                let sv1_submit = submit_recv.clone().recv().await.unwrap();
                let sv2_submit: SubmitSharesExtended = todo!();
                let submit_to_sv2 = self_.safe_lock(|s| s.submit_to_sv2.clone()).unwrap();
                submit_to_sv2.send(sv2_submit).await.unwrap();
            }
        });
    }

    fn handle_new_prev_hash(self_: Arc<Mutex<Self>>) {
        //TODO!
        task::spawn(async {loop {}});
    }

    fn handle_new_extended_minig_job(self_: Arc<Mutex<Self>>) {
        //TODO!
        task::spawn(async {loop {}});
    }
}


