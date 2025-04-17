pub mod error;
mod inner;
mod message;
mod response;

use crate::channel_management::id::{
    error::ChannelIdFactoryError,
    inner::{InnerChannelIdFactory, InnerChannelIdFactoryIo},
    message::InnerChannelIdFactoryMessage,
    response::InnerChannelIdFactoryResponse,
};

use std::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ChannelIdFactory {
    message_sender_into_inner_factory: mpsc::Sender<InnerChannelIdFactoryIo>,
}

impl ChannelIdFactory {
    /// Creates a new factory and starts its background thread for managing the internal state under
    /// the Actor Model.
    ///
    /// This is the main entry point for creating and starting an `ChannelIdFactory`.
    pub fn new() -> Self {
        let (message_sender, message_receiver) = mpsc::channel();
        let inner_factory = InnerChannelIdFactory::new(message_receiver);

        // Start the background task
        tokio::task::spawn_blocking(move || {
            inner_factory.run();
        });

        Self {
            message_sender_into_inner_factory: message_sender,
        }
    }

    // Private method for communicating with the inner factory actor and returning the response
    fn inner_factory_io(
        &self,
        message: InnerChannelIdFactoryMessage,
    ) -> Result<InnerChannelIdFactoryResponse, ChannelIdFactoryError> {
        let (response_sender, response_receiver) = mpsc::channel();

        self.message_sender_into_inner_factory
            .send(InnerChannelIdFactoryIo::new(message, response_sender))
            .map_err(|_| ChannelIdFactoryError::MessageSenderError)?;

        response_receiver
            .recv()
            .map_err(|_| ChannelIdFactoryError::ResponseReceiverError)
    }

    /// Shuts down the factory runner task.
    pub fn shutdown(&self) -> Result<(), ChannelIdFactoryError> {
        let response = self.inner_factory_io(InnerChannelIdFactoryMessage::Shutdown)?;
        match response {
            InnerChannelIdFactoryResponse::Shutdown => Ok(()),
            _ => Err(ChannelIdFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the next channel id on the factory.
    pub fn next(&self) -> Result<u32, ChannelIdFactoryError> {
        let response = self.inner_factory_io(InnerChannelIdFactoryMessage::NextId)?;
        match response {
            InnerChannelIdFactoryResponse::NextId(id) => Ok(id),
            _ => Err(ChannelIdFactoryError::UnexpectedResponse),
        }
    }
}
