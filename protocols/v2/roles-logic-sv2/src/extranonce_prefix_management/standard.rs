use crate::extranonce_prefix_management::{
    error::ExtranoncePrefixFactoryError,
    inner::{InnerExtranoncePrefixFactory, InnerExtranoncePrefixFactoryIo},
    message::InnerExtranoncePrefixFactoryMessage,
    response::InnerExtranoncePrefixFactoryResponse,
};

use std::{ops::Range, sync::mpsc};
use tokio;

/// A Factory for creating unique extranonce prefixes in a under the [Actor Model](https://en.wikipedia.org/wiki/Actor_model).
///
/// It can be shared across multiple `StandardChannelFactory` instances while guaranteeing unique
/// `extranonce_prefix` allocation.
///
/// Not suitable for `ExtendedChannelFactory` instances.
#[derive(Clone)]
pub struct ExtranoncePrefixFactoryStandard {
    message_sender_into_inner_factory: mpsc::Sender<InnerExtranoncePrefixFactoryIo>,
}

impl ExtranoncePrefixFactoryStandard {
    pub fn new(
        standard_extranonce_range_0: Range<usize>,
        standard_extranonce_range_1: Range<usize>,
        standard_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
    ) -> Result<Self, ExtranoncePrefixFactoryError> {
        let (message_sender, message_receiver) = mpsc::channel();

        let inner_factory = InnerExtranoncePrefixFactory::new(
            message_receiver,
            standard_extranonce_range_0,
            standard_extranonce_range_1,
            standard_extranonce_range_2,
            additional_coinbase_script_data,
        )
        .map_err(ExtranoncePrefixFactoryError::FailedToCreateInnerFactory)?;

        tokio::task::spawn_blocking(move || {
            inner_factory.run();
        });

        Ok(Self {
            message_sender_into_inner_factory: message_sender,
        })
    }

    /// Sends a message to the inner factory Actor and returns the response.
    fn inner_factory_io(
        &self,
        message: InnerExtranoncePrefixFactoryMessage,
    ) -> Result<InnerExtranoncePrefixFactoryResponse, ExtranoncePrefixFactoryError> {
        let (response_sender, response_receiver) = mpsc::channel();
        self.message_sender_into_inner_factory
            .send(InnerExtranoncePrefixFactoryIo::new(
                message,
                response_sender,
            ))
            .map_err(|_| ExtranoncePrefixFactoryError::MessageSenderError)?;
        response_receiver
            .recv()
            .map_err(|_| ExtranoncePrefixFactoryError::MessageSenderError)
    }

    /// Sends a shutdown message to the inner factory Actor and returns the response.
    pub fn shutdown(&self) -> Result<(), ExtranoncePrefixFactoryError> {
        let response = self.inner_factory_io(InnerExtranoncePrefixFactoryMessage::Shutdown)?;

        match response {
            InnerExtranoncePrefixFactoryResponse::Shutdown => Ok(()),
            _ => Err(ExtranoncePrefixFactoryError::UnexpectedResponse),
        }
    }

    /// Requests the next standard extranonce prefix from the inner factory Actor.
    pub fn next_prefix_standard(&self) -> Result<Vec<u8>, ExtranoncePrefixFactoryError> {
        let response =
            self.inner_factory_io(InnerExtranoncePrefixFactoryMessage::NextPrefixStandard)?;

        match response {
            InnerExtranoncePrefixFactoryResponse::NextPrefixStandard(extranonce_prefix) => {
                Ok(extranonce_prefix)
            }
            InnerExtranoncePrefixFactoryResponse::FailedToGenerateNextExtranoncePrefixStandard(
                e,
            ) => Err(ExtranoncePrefixFactoryError::FailedToGenerateNextExtranoncePrefixStandard(e)),
            _ => Err(ExtranoncePrefixFactoryError::UnexpectedResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, sync::Arc};
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn test_concurrent_allocation_of_extranonce_prefix_standard() {
        // Create the factory with sample ranges
        let factory = Arc::new(
            ExtranoncePrefixFactoryStandard::new(
                0..0,  // range_0
                0..7,  // range_1
                7..32, // range_2
                None,  // additional_coinbase_script_data
            )
            .expect("Failed to create factory"),
        );

        // Generate standard prefixes concurrently
        let num_requests = 20;

        // Create a tokio barrier to synchronize all tasks
        let barrier = Arc::new(Barrier::new(num_requests));

        // Use JoinSet to spawn and collect tasks
        let mut join_set = tokio::task::JoinSet::new();

        for _ in 0..num_requests {
            let factory_clone = Arc::clone(&factory);
            let barrier_clone = Arc::clone(&barrier);
            join_set.spawn(async move {
                // Wait for all tasks to reach this point before proceeding
                barrier_clone.wait().await;
                factory_clone.next_prefix_standard()
            });
        }

        // Collect results
        let mut standard_prefixes = Vec::with_capacity(num_requests);
        while let Some(result) = join_set.join_next().await {
            if let Ok(prefix_result) = result {
                if let Ok(prefix) = prefix_result {
                    println!("standard prefix: {:?}", prefix);
                    standard_prefixes.push(prefix);
                } else {
                    println!("error: {:?}", prefix_result);
                }
            } else {
                println!("task join error");
            }
        }

        // Verify we got the expected number of prefixes
        assert_eq!(
            standard_prefixes.len(),
            num_requests,
            "Should have received all standard prefixes"
        );

        // Verify all standard prefixes are unique
        let unique_standard: HashSet<_> = standard_prefixes.iter().collect();
        assert_eq!(
            unique_standard.len(),
            standard_prefixes.len(),
            "All standard prefixes should be unique"
        );

        // Shutdown the factory
        factory.shutdown().expect("Failed to shut down factory");
    }
}
