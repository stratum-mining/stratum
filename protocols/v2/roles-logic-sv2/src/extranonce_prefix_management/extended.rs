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
/// It can be shared across multiple `ExtendedChannelFactory` instances while guaranteeing unique
/// `extranonce_prefix` allocation.
///
/// Not suitable for `StandardChannelFactory` instances.
#[derive(Clone)]
pub struct ExtranoncePrefixFactoryExtended {
    message_sender_into_inner_factory: mpsc::Sender<InnerExtranoncePrefixFactoryIo>,
    rollable_extranonce_size: u16,
}

impl ExtranoncePrefixFactoryExtended {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
    ) -> Result<Self, ExtranoncePrefixFactoryError> {
        let (message_sender, message_receiver) = mpsc::channel();

        let inner_factory = InnerExtranoncePrefixFactory::new(
            message_receiver,
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2.clone(),
            additional_coinbase_script_data,
        )
        .map_err(ExtranoncePrefixFactoryError::FailedToCreateInnerFactory)?;

        tokio::task::spawn_blocking(move || {
            inner_factory.run();
        });

        Ok(Self {
            message_sender_into_inner_factory: message_sender,
            rollable_extranonce_size: (extended_extranonce_range_2.end
                - extended_extranonce_range_2.start) as u16,
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

    /// Requests the next extended extranonce prefix from the inner factory Actor.
    pub fn next_prefix_extended(
        &self,
        rollable_extranonce_size: usize,
    ) -> Result<Vec<u8>, ExtranoncePrefixFactoryError> {
        let response = self.inner_factory_io(
            InnerExtranoncePrefixFactoryMessage::NextPrefixExtended(rollable_extranonce_size),
        )?;

        match response {
            InnerExtranoncePrefixFactoryResponse::NextPrefixExtended(extranonce_prefix) => {
                Ok(extranonce_prefix)
            }
            InnerExtranoncePrefixFactoryResponse::FailedToGenerateNextExtranoncePrefixExtended(
                e,
            ) => Err(ExtranoncePrefixFactoryError::FailedToGenerateNextExtranoncePrefixExtended(e)),
            _ => Err(ExtranoncePrefixFactoryError::UnexpectedResponse),
        }
    }

    pub fn get_rollable_extranonce_size(&self) -> u16 {
        self.rollable_extranonce_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, sync::Arc};
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn test_concurrent_allocation_of_extranonce_prefix_extended() {
        // Create the factory with sample ranges
        let factory = Arc::new(
            ExtranoncePrefixFactoryExtended::new(
                0..0,  // range_0
                0..7,  // range_1
                7..32, // range_2
                None,  // additional_coinbase_script_data
            )
            .expect("Failed to create factory"),
        );

        // Generate extended prefixes concurrently
        let num_requests = 20;

        // Create a tokio barrier to synchronize all tasks
        let barrier = Arc::new(Barrier::new(num_requests));

        // Use JoinSet to spawn and collect tasks
        let mut join_set = tokio::task::JoinSet::new();

        for i in 0..num_requests {
            let factory_clone = Arc::clone(&factory);
            let barrier_clone = Arc::clone(&barrier);

            join_set.spawn(async move {
                // Wait for all tasks to reach this point before proceeding
                barrier_clone.wait().await;

                // Use different rollable sizes to test different scenarios
                let rollable_size = if i % 2 == 0 { 4 } else { 3 };
                factory_clone.next_prefix_extended(rollable_size)
            });
        }

        // Collect results
        let mut extended_prefixes = Vec::with_capacity(num_requests);
        while let Some(result) = join_set.join_next().await {
            if let Ok(prefix_result) = result {
                if let Ok(prefix) = prefix_result {
                    println!("extended prefix: {:?}", prefix);
                    extended_prefixes.push(prefix);
                } else {
                    println!("error: {:?}", prefix_result);
                }
            } else {
                println!("task join error");
            }
        }

        // Verify we got the expected number of prefixes
        assert_eq!(
            extended_prefixes.len(),
            num_requests,
            "Should have received all extended prefixes"
        );

        // Verify all extended prefixes are unique
        let unique_extended: HashSet<_> = extended_prefixes.iter().collect();
        assert_eq!(
            unique_extended.len(),
            extended_prefixes.len(),
            "All extended prefixes should be unique"
        );

        // Shutdown the factory
        factory.shutdown().expect("Failed to shut down factory");
    }

    #[tokio::test]
    async fn test_range_1_size_equal_to_additional_coinbase_script_data_length() {
        // Create the factory with sample ranges
        let factory = ExtranoncePrefixFactoryExtended::new(
            0..0, // range_0
            0..7, /* range_1 (equal to the length of the additional coinbase script data,
                   * leaving no space to be rolled) */
            7..32,                     // range_2
            Some(b"bitaloo".to_vec()), // additional_coinbase_script_data
        )
        .expect("Failed to create factory");

        // result: Err(FailedToGenerateNextExtranoncePrefixExtended(MaxValueReached))
        let result = factory.next_prefix_extended(4);

        println!("result: {:?}", result);

        assert!(result.is_err());

        // Shutdown the factory
        factory.shutdown().expect("Failed to shut down factory");
    }
}
