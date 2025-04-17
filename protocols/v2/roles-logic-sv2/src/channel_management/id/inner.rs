use crate::{
    channel_management::id::{
        message::InnerChannelIdFactoryMessage, response::InnerChannelIdFactoryResponse,
    },
    utils::Id as IdFactory,
};

use tracing::error;

use std::sync::mpsc;

/// Encapsulates the Input/Output interface of the inner factory (Actor Model implementation of the
/// `ChannelIdFactory`).
pub struct InnerChannelIdFactoryIo {
    message: InnerChannelIdFactoryMessage,
    response_sender: mpsc::Sender<InnerChannelIdFactoryResponse>,
}

impl InnerChannelIdFactoryIo {
    pub fn new(
        message: InnerChannelIdFactoryMessage,
        response_sender: mpsc::Sender<InnerChannelIdFactoryResponse>,
    ) -> Self {
        Self {
            message,
            response_sender,
        }
    }
}

pub struct InnerChannelIdFactory {
    message_receiver: mpsc::Receiver<InnerChannelIdFactoryIo>,
    channel_id_factory: IdFactory,
}

impl InnerChannelIdFactory {
    pub fn new(message_receiver: mpsc::Receiver<InnerChannelIdFactoryIo>) -> Self {
        Self {
            message_receiver,
            channel_id_factory: IdFactory::new(),
        }
    }

    pub fn run(mut self) {
        loop {
            match self.message_receiver.recv() {
                Ok(io) => {
                    let InnerChannelIdFactoryIo {
                        message,
                        response_sender,
                    } = io;

                    match message {
                        InnerChannelIdFactoryMessage::Shutdown => {
                            if response_sender
                                .send(InnerChannelIdFactoryResponse::Shutdown)
                                .is_err()
                            {
                                error!("ChannelIdFactory response sender closed");
                                break;
                            }
                            break;
                        }
                        InnerChannelIdFactoryMessage::NextId => {
                            let id = self.channel_id_factory.next();
                            if response_sender
                                .send(InnerChannelIdFactoryResponse::NextId(id))
                                .is_err()
                            {
                                error!("ChannelIdFactory response sender closed");
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    error!("ChannelIdFactory message receiver closed");
                    break;
                }
            }
        }
    }
}
