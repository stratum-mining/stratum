use crate::upstream_sv2::EitherFrame;
use async_std::net::TcpStream;

use async_channel::{bounded, Receiver, Sender};
use async_std::{io::BufReader, prelude::*, task};
use roles_logic_sv2::utils::Mutex;
use std::sync::Arc;
use v1::json_rpc;

pub(crate) struct Translator {
    /// Sends Sv2 messages  to the upstream. these sv2 messages were receieved from
    /// reciever_downstream and then translated from sv1 to sv2
    pub(crate) sender_upstream: Sender<EitherFrame>,
    /// Receives Sv2 messages from upstream to be translated into Sv1 and sent to downstream via
    /// the sender_downstream
    /// will have the other part of the channel on the upstream that wont be called sender_upstream
    /// (becuase we have sender_upstream here), the other part of the channel that lives on
    /// Upstream will be called sender_upstream
    pub(crate) receiver_upstream: Receiver<EitherFrame>,
    /// Sends Sv1 messages from initially received by reciever_upstream, then translated to Sv1 and
    /// then will be received by reciver_downstream
    pub(crate) sender_downstream: Sender<json_rpc::Message>,
    /// Receives Sv1 messages from the sender_downstream to be translated to Sv2 and sent to the
    /// sender_upstream
    pub(crate) receiver_downstream: Receiver<json_rpc::Message>,
}

impl Translator {
    pub(crate) fn new() -> Self {
        let (sender_upstream, receiver_upstream) = bounded(10);
        let (sender_downstream, receiver_downstream) = bounded(10);
        Translator {
            sender_upstream,
            receiver_upstream,
            sender_downstream,
            receiver_downstream,
        }
    }
}
