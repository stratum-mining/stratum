use mining_sv2::{ExtendedExtranonce, ExtendedExtranonceError};
use std::{ops::Range, sync::mpsc};

use crate::extranonce_prefix_management::{
    message::InnerExtranoncePrefixFactoryMessage, response::InnerExtranoncePrefixFactoryResponse,
};

use tracing::{debug, error};

/// Encapsulates the Input/Output interface of the inner factory (Actor Model implementation of
/// `ExtranoncePrefixFactoryStandard` and `ExtranoncePrefixFactoryExtended`).
pub struct InnerExtranoncePrefixFactoryIo {
    message: InnerExtranoncePrefixFactoryMessage,
    response_sender: mpsc::Sender<InnerExtranoncePrefixFactoryResponse>,
}

impl InnerExtranoncePrefixFactoryIo {
    pub fn new(
        message: InnerExtranoncePrefixFactoryMessage,
        response_sender: mpsc::Sender<InnerExtranoncePrefixFactoryResponse>,
    ) -> Self {
        Self {
            message,
            response_sender,
        }
    }
}

/// Actor model implementation of the `ExtranoncePrefixFactoryStandard`.
///
/// Encapsulates the Extranonce Prefix Factory state and logic in a concurrency-safe way.
///
/// The `run` method handles incoming messages from the public methods of
/// `ExtranoncePrefixFactoryStandard`, which always arrive as instances of
/// `InnerExtranoncePrefixFactoryIo`.
pub struct InnerExtranoncePrefixFactory {
    message_receiver: mpsc::Receiver<InnerExtranoncePrefixFactoryIo>,
    extended_extranonce: ExtendedExtranonce,
}

impl InnerExtranoncePrefixFactory {
    pub fn new(
        message_receiver: mpsc::Receiver<InnerExtranoncePrefixFactoryIo>,
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
    ) -> Result<Self, ExtendedExtranonceError> {
        let extended_extranonce = ExtendedExtranonce::new(
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2,
            additional_coinbase_script_data,
        )?;
        Ok(Self {
            message_receiver,
            extended_extranonce,
        })
    }

    pub fn run(mut self) {
        loop {
            match self.message_receiver.recv() {
                Ok(message_io) => match message_io.message {
                    InnerExtranoncePrefixFactoryMessage::Shutdown => {
                        debug!("Shutting down extranonce prefix factory");
                        let response = InnerExtranoncePrefixFactoryResponse::Shutdown;
                        if let Err(e) = message_io.response_sender.send(response) {
                            error!("Failed to send response: {:?}", e);
                            break;
                        }
                        break;
                    }
                    InnerExtranoncePrefixFactoryMessage::NextPrefixExtended(
                        rollable_extranonce_size,
                    ) => {
                        let response = self.handle_next_prefix_extended(rollable_extranonce_size);

                        if let Err(e) = message_io.response_sender.send(response) {
                            error!("Failed to send response: {:?}", e);
                            break;
                        }
                    }
                    InnerExtranoncePrefixFactoryMessage::NextPrefixStandard => {
                        let response = self.handle_next_prefix_standard();

                        if let Err(e) = message_io.response_sender.send(response) {
                            error!("Failed to send response: {:?}", e);
                            break;
                        }
                    }
                },
                Err(_) => {
                    // Channel closed, exit the loop
                    break;
                }
            }
        }
    }

    fn handle_next_prefix_extended(
        &mut self,
        rollable_extranonce_size: usize,
    ) -> InnerExtranoncePrefixFactoryResponse {
        match self
            .extended_extranonce
            .next_prefix_extended(rollable_extranonce_size)
        {
            Ok(prefix) => InnerExtranoncePrefixFactoryResponse::NextPrefixExtended(prefix.into()),
            Err(e) => {
                InnerExtranoncePrefixFactoryResponse::FailedToGenerateNextExtranoncePrefixExtended(
                    e,
                )
            }
        }
    }

    fn handle_next_prefix_standard(&mut self) -> InnerExtranoncePrefixFactoryResponse {
        match self.extended_extranonce.next_prefix_standard() {
            Ok(prefix) => InnerExtranoncePrefixFactoryResponse::NextPrefixStandard(prefix.into()),
            Err(e) => {
                InnerExtranoncePrefixFactoryResponse::FailedToGenerateNextExtranoncePrefixStandard(
                    e,
                )
            }
        }
    }
}
