use crate::{
    net::{setup_as_downstream, setup_as_upstream},
    Action, ActionResult, Role, Test,
};
use async_channel::{Receiver, Sender};
use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::StandardEitherFrame as EitherFrame;

pub struct Executor<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> {
    send_to_down: Option<Sender<EitherFrame<Message>>>,
    recv_from_down: Option<Receiver<EitherFrame<Message>>>,
    send_to_up: Option<Sender<EitherFrame<Message>>>,
    recv_from_up: Option<Receiver<EitherFrame<Message>>>,
    actions: Vec<Action<Message>>,
}

impl<Message: Serialize + Deserialize<'static> + GetSize + Send + 'static> Executor<Message> {
    pub async fn new(test: Test<Message>) -> Self {
        match (test.as_dowstream, test.as_upstream) {
            (Some(as_down), Some(as_up)) => {
                let (recv_from_down, send_to_down) =
                    setup_as_upstream(as_up.addr, as_up.keys).await;
                let (recv_from_up, send_to_up) =
                    setup_as_downstream(as_down.addr, as_down.key).await;
                Self {
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions,
                }
            }
            (None, Some(as_up)) => {
                let (recv_from_down, send_to_down) =
                    setup_as_upstream(as_up.addr, as_up.keys).await;
                Self {
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: None,
                    recv_from_up: None,
                    actions: test.actions,
                }
            }
            (Some(as_down), None) => {
                let (recv_from_up, send_to_up) =
                    setup_as_downstream(as_down.addr, as_down.key).await;
                Self {
                    send_to_down: None,
                    recv_from_down: None,
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions,
                }
            }
            (None, None) => todo!(),
        }
    }

    pub async fn execute(self) -> bool {
        for action in self.actions {
            let (sender, receiver) = match action.role {
                Role::Upstream => (
                    self.send_to_down
                        .as_ref()
                        .expect("Action require executor to act as upstream"),
                    self.recv_from_down
                        .as_ref()
                        .expect("Action require executor to act as upstream"),
                ),
                Role::Downstream => (
                    self.send_to_up
                        .as_ref()
                        .expect("Action require executor to act as downstream"),
                    self.recv_from_up
                        .as_ref()
                        .expect("Action require executor to act as downstream"),
                ),
            };
            for message in action.messages {
                match sender.send(message).await {
                    Ok(_) => (),
                    Err(_) => {
                        if action.result == ActionResult::CloseConnection {
                            return true;
                        } else {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}
