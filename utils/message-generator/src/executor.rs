use crate::{Action,ActionResult,Role,Test};
use async_channel::{Receiver, Sender};
use binary_sv2::{Deserialize, GetSize, Serialize};

pub struct Executor <
    Message: Serialize + Deserialize<'static> + GetSize + Send + 'static,
> {
    send_to_downstream: Sender<Message>,
    receive_from_dowstream: Receiver<Message>,
    send_to_upstream: Sender<Message>,
    receive_from_upstream: Receiver<Message>,
    test: Test<Message>,
}
