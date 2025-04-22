pub mod error;
mod inner;
mod message;
mod response;

use crate::{
    channel_management::{
        id::ChannelIdFactory,
        share_accounting::{ShareAccounting, ShareValidationResult},
        standard::{
            channel::{group::GroupChannel, standard::StandardChannel},
            factory::{
                error::StandardChannelFactoryError,
                inner::{InnerStandardChannelFactory, InnerStandardChannelFactoryIo},
                message::InnerStandardChannelFactoryMessage,
                response::InnerStandardChannelFactoryResponse,
            },
        },
    },
    extranonce_prefix_management::standard::ExtranoncePrefixFactoryStandard,
    job_management::chain_tip::ChainTip,
};
use binary_sv2::U256;
use mining_sv2::SubmitSharesStandard;
use std::{collections::HashMap, sync::mpsc};
use stratum_common::bitcoin::transaction::TxOut;
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

/// A Factory for creating and managing [`StandardChannel`] and [`GroupChannel`] instances under the [Actor Model](https://en.wikipedia.org/wiki/Actor_model).
///
/// In other words: it spawns a background `tokio` task that handles all state-changes in a
/// concurrency-safe manner.
///
/// Only suitable for mining servers, not clients.
///
/// Allows the user to manage channel states upon receipt of the following Sv2 messages:
/// - Mining Protocol:
///   - `OpenStandardMiningChannel`: create new standard channel
///   - `UpdateChannel`: update standard channel nominal hashrate and target
///   - `CloseChannel`: remove standard channel
///   - `SubmitSharesStandard`: validate shares
/// - Template Distribution Protocol:
///   - `NewTemplate`: update channel jobs
///   - `SetNewPrevHash`: update chain tip for share validation, activate future jobs
///
/// The decision of which group channel a standard channel belongs to is up to the caller.
///
/// The user is responsible for creating the appropriate Sv2 messages to be sent to the client upon
/// state changes triggered by methods calls. Methods return all necessary information for creating
/// those messages.
///
/// If the `shutdown` method is not called, the caller is under risk of leaving behind a zombie
#[derive(Debug, Clone)]
pub struct StandardChannelFactory {
    message_sender_into_inner_factory: mpsc::Sender<InnerStandardChannelFactoryIo>,
}

impl StandardChannelFactory {
    pub fn new(
        extranonce_prefix_factory: ExtranoncePrefixFactoryStandard,
        expected_share_per_minute_per_channel: f32,
        share_batch_size: usize,
        channel_id_factory: ChannelIdFactory,
    ) -> Self {
        let (message_sender, message_receiver) = mpsc::channel();

        let inner_factory = InnerStandardChannelFactory::new(
            message_receiver,
            extranonce_prefix_factory,
            expected_share_per_minute_per_channel,
            share_batch_size,
            channel_id_factory,
        );

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
        message: InnerStandardChannelFactoryMessage<'static>,
    ) -> Result<InnerStandardChannelFactoryResponse<'static>, StandardChannelFactoryError> {
        // Create a oneshot channel for the response
        let (response_sender, response_receiver) = mpsc::channel();

        // Send the message to the inner factory
        self.message_sender_into_inner_factory
            .send(InnerStandardChannelFactoryIo::new(message, response_sender))
            .map_err(|_| StandardChannelFactoryError::MessageSenderError)?;

        // Wait for the response
        response_receiver
            .recv()
            .map_err(|_| StandardChannelFactoryError::ResponseReceiverError)
    }

    /// Shuts down the factory runner thread.
    pub fn shutdown(&self) -> Result<(), StandardChannelFactoryError> {
        // Send the shutdown message and wait for the response
        let response = self.inner_factory_io(InnerStandardChannelFactoryMessage::Shutdown)?;

        match response {
            InnerStandardChannelFactoryResponse::Shutdown => Ok(()),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Creates a new standard channel. To be called upon receipt of a `OpenStandardMiningChannel`
    /// Sv2 message.
    ///
    /// The caller is responsible for deciding which `group_channel_id` will be used to assign the
    /// new channel.
    ///
    /// On success, it returns:
    /// - The channel id of the new channel
    /// - The target of the new channel
    /// - The extranonce prefix of the new channel
    ///
    /// These values should be used by the caller to craft a OpenStandardMiningChannel.Success
    ///
    /// On error, it returns a `StandardChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `RequestedMaxTargetTooHigh` (which should be used by the caller to craft a
    ///   `OpenMiningChannel.Error` message with `max_target_out_of_range` as `error_code)
    ///
    /// Other error variants should be treated as some internal error, but not necessarily something
    /// to be reported via `OpenMiningChannel.Error`.
    pub fn new_standard_channel(
        &self,
        user_identity: String,
        nominal_hashrate: f32,
        max_target: U256<'static>,
        group_channel_id: u32,
    ) -> Result<(u32, U256<'static>, Vec<u8>), StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::NewStandardChannel(
                user_identity,
                nominal_hashrate,
                max_target,
                group_channel_id,
            ))?;

        match response {
            InnerStandardChannelFactoryResponse::NewStandardChannelCreated(
                channel_id,
                target,
                extranonce_prefix,
            ) => Ok((channel_id, target, extranonce_prefix)),
            InnerStandardChannelFactoryResponse::RequestedMaxTargetOutOfRange => {
                // caller should match against this error variant
                // in order to properly craft a `OpenMiningChannel.Error` message
                // with max-target-out-of-range as error_code
                Err(StandardChannelFactoryError::RequestedMaxTargetOutOfRange)
            }
            InnerStandardChannelFactoryResponse::InvalidNominalHashrate => {
                Err(StandardChannelFactoryError::InvalidNominalHashrate)
            }
            InnerStandardChannelFactoryResponse::FailedToGenerateNextExtranoncePrefixStandard(
                e,
            ) => Err(StandardChannelFactoryError::FailedToGenerateNextExtranoncePrefixStandard(e)),
            InnerStandardChannelFactoryResponse::FailedToGenerateNextChannelId(e) => Err(
                StandardChannelFactoryError::FailedToGenerateNextChannelId(e),
            ),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of the standard channel with the specified id, if it exists.
    ///
    /// Any modifications made to the returned channel will not affect the original channel
    /// stored in the factory, as it returns a completely independent copy.
    pub fn get_standard_channel(
        &self,
        channel_id: u32,
    ) -> Result<StandardChannel, StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::GetStandardChannel(channel_id),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::StandardChannel(channel) => Ok(channel),
            InnerStandardChannelFactoryResponse::StandardChannelIdNotFound => {
                Err(StandardChannelFactoryError::StandardChannelNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the number of standard channels currently in the factory.
    pub fn get_standard_channel_count(&self) -> Result<u32, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::GetStandardChannelCount)?;

        match response {
            InnerStandardChannelFactoryResponse::StandardChannelCount(count) => Ok(count),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of all standard channels currently in the factory.
    ///
    /// Any modifications made to the returned channels will not affect the original channels
    /// stored in the factory, as it returns a completely independent copy.
    pub fn get_all_standard_channels(
        &self,
    ) -> Result<HashMap<u32, StandardChannel>, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::GetAllStandardChannels)?;

        match response {
            InnerStandardChannelFactoryResponse::AllStandardChannels(channels) => Ok(channels),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Removes the channel with the specified id, if it exists.
    pub fn remove_standard_channel(
        &self,
        channel_id: u32,
    ) -> Result<(), StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::RemoveStandardChannel(channel_id),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::StandardChannelRemoved => Ok(()),
            InnerStandardChannelFactoryResponse::StandardChannelIdNotFound => {
                Err(StandardChannelFactoryError::StandardChannelNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Updates the standard channel with the specified id, if it exists.
    ///
    /// On success, it returns the new max target of the channel. This shall be used by the caller
    /// to craft a `SetTarget` Sv2 message to be sent to the client across the wire.
    ///
    /// On error, it returns a `StandardChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `RequestedMaxTargetOutOfRange` (which should be used by the caller to craft a
    ///   `UpdateChannel.Error` message with `max_target_out_of_range` as `error_code`)
    pub fn update_standard_channel(
        &self,
        channel_id: u32,
        nominal_hashrate: f32,
        max_target: U256<'static>,
    ) -> Result<U256<'static>, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::UpdateStandardChannel(
                channel_id,
                nominal_hashrate,
                max_target,
            ))?;

        match response {
            InnerStandardChannelFactoryResponse::StandardChannelUpdated(new_max_target) => {
                Ok(new_max_target)
            }
            InnerStandardChannelFactoryResponse::StandardChannelIdNotFound => {
                Err(StandardChannelFactoryError::StandardChannelNotFound)
            }
            InnerStandardChannelFactoryResponse::RequestedMaxTargetOutOfRange => {
                Err(StandardChannelFactoryError::RequestedMaxTargetOutOfRange)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Creates a new group channel. Should be called at least once before creating any standard
    /// channels, as all standard channels must be part of some group channel.
    ///
    /// On success, it returns the id of the new group channel.
    pub fn new_group_channel(&self) -> Result<u32, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::NewGroupChannel)?;

        match response {
            InnerStandardChannelFactoryResponse::NewGroupChannelCreated(channel_id) => {
                Ok(channel_id)
            }
            InnerStandardChannelFactoryResponse::ProcessNewTemplateGroupChannelError(e) => {
                Err(StandardChannelFactoryError::ProcessNewTemplateGroupChannelError(e))
            }
            InnerStandardChannelFactoryResponse::FailedToGenerateNextChannelId(e) => Err(
                StandardChannelFactoryError::FailedToGenerateNextChannelId(e),
            ),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Removes the group channel with the specified id, if it exists.
    ///
    /// Also removes all standard channels that are part of the group channel.
    pub fn remove_group_channel(
        &self,
        group_channel_id: u32,
    ) -> Result<(), StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::RemoveGroupChannel(group_channel_id),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::GroupChannelRemoved => Ok(()),
            InnerStandardChannelFactoryResponse::GroupChannelIdNotFound => {
                Err(StandardChannelFactoryError::GroupChannelNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of the group channel with the specified id, if it exists.
    ///
    /// Any modifications made to the returned group channel will not affect the original group
    /// channel stored in the factory, as it returns a completely independent copy.
    pub fn get_group_channel(
        &self,
        group_channel_id: u32,
    ) -> Result<GroupChannel, StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::GetGroupChannel(group_channel_id),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::GroupChannel(group_channel) => Ok(group_channel),
            InnerStandardChannelFactoryResponse::GroupChannelIdNotFound => {
                Err(StandardChannelFactoryError::GroupChannelNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the number of group channels currently in the factory.
    pub fn get_group_channel_count(&self) -> Result<u32, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::GetGroupChannelCount)?;

        match response {
            InnerStandardChannelFactoryResponse::GroupChannelCount(count) => Ok(count),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of all group channels currently in the factory.
    ///
    /// Any modifications made to the returned group channels will not affect the original group
    /// channels stored in the factory, as it returns a completely independent copy.
    pub fn get_all_group_channels(
        &self,
    ) -> Result<HashMap<u32, GroupChannel>, StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::GetAllGroupChannels)?;

        match response {
            InnerStandardChannelFactoryResponse::AllGroupChannels(channels) => Ok(channels),
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Processes a new template.
    ///
    /// To be called by mining servers upon receipt of a `NewTemplate` message of the Sv2 Template
    /// Distribution Protocol.
    ///
    /// The caller is responsible for providing the coinbase reward outputs that sum to the
    /// value in the template, which will be used during creation of the new jobs associated with
    /// this new template.
    ///
    /// Calling this method effectively updates the state of all channels in the factory.
    /// If the template is a future template, a new future job is created on each channel.
    /// If the template is not a future template, a new active job is created on each channel,
    /// while the previously active job is moved to the past jobs of the channel.
    ///
    /// Group channels get new extended jobs, while standard channels get new standard jobs
    /// that are created by converting the extended jobs of the group channel they belong to.
    ///
    /// Note that this simply updates the state of the channels. After calling this method,
    /// the caller is still responsible for querying the state of each channel in order to craft
    /// the appropriate Sv2 messages to be sent to the clients across the wire (e.g.:
    /// `NewExtendedMiningJob` for group channels and `NewMiningJob` for standard channels).
    pub fn process_new_template(
        &self,
        template: NewTemplate<'static>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), StandardChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerStandardChannelFactoryMessage::ProcessNewTemplate(
                template,
                coinbase_reward_outputs,
            ))?;

        match response {
            InnerStandardChannelFactoryResponse::ProcessedNewTemplate => Ok(()),
            InnerStandardChannelFactoryResponse::ChainTipNotSet => {
                Err(StandardChannelFactoryError::ChainTipNotSet)
            }
            InnerStandardChannelFactoryResponse::InvalidCoinbaseRewardOutputs => {
                Err(StandardChannelFactoryError::InvalidCoinbaseRewardOutputs)
            }
            InnerStandardChannelFactoryResponse::ProcessNewTemplateGroupChannelError(e) => {
                Err(StandardChannelFactoryError::ProcessNewTemplateGroupChannelError(e))
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Processes a new chain tip update.
    ///
    /// To be called by mining servers upon receipt of a `SetNewPrevHash` of the Template
    /// Distribution Protocol.
    ///
    /// It effectively updates the chain tip for all channels in the factory.
    ///
    /// If a NewTemplate (with future_template = true) was previously processed
    /// (leading to the creation of a corresponding future job), the `template_id`
    /// on the incoming `SetNewPrevHash` message will effectively move the
    /// corresponding future job into the status of the active job of the channel.
    ///
    /// All past jobs are cleared, and shares submitted for them are rejected from this point on.
    ///
    /// Note that this simply updates the state of the channels. After calling this method,
    /// the caller is still responsible for querying the state of each channel in order to craft
    /// the appropriate Sv2 messages to be sent to the clients across the wire (e.g.:
    /// `NewExtendedMiningJob`).
    pub fn process_set_new_prev_hash(
        &self,
        set_new_prev_hash: SetNewPrevHash<'static>,
    ) -> Result<(), StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::ProcessSetNewPrevHash(set_new_prev_hash),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::ProcessedSetNewPrevHash => Ok(()),
            InnerStandardChannelFactoryResponse::TemplateNotFound => {
                Err(StandardChannelFactoryError::TemplateNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Validates a share.
    ///
    /// To be called by mining servers upon receipt of a `SubmitSharesStandard` message.
    ///
    /// It updates the state of standard channels with the appropriate share accounting.
    ///
    /// On success, it returns a `ShareValidationResult` that can be used by the caller to decide
    /// what to do. Namely, the following variants are possible:
    /// - `Valid`: The share is valid and the state of the channel has been updated accordingly.
    /// - `ValidWithAcknowledgement(last_sequence_number, new_submits_accepted_count,
    ///   new_shares_sum)`: The share is valid and it's time to send an acknowledgement to the
    ///   client (e.g.: `SubmitShares.Success`).
    /// - `BlockFound(template_id, coinbase)`: A block has been found. `template_id` is None if the
    ///   share is for a custom job.
    ///
    /// On error, it returns a `StandardChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `InvalidShare`: The share is invalid and the state of the channel has not been updated.
    /// - `StaleShare`: The share is stale and the state of the channel has not been updated.
    /// - `InvalidJobId`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   invalid-job-id`.
    /// - `ShareDoesNotMeetTarget`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   difficulty-too-low`.
    /// - `DuplicateShare`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   duplicate-share`.
    pub fn validate_share(
        &self,
        submit_shares_standard: SubmitSharesStandard,
    ) -> Result<ShareValidationResult, StandardChannelFactoryError> {
        let response = self.inner_factory_io(InnerStandardChannelFactoryMessage::ValidateShare(
            submit_shares_standard,
        ))?;

        match response {
            InnerStandardChannelFactoryResponse::ValidShare => Ok(ShareValidationResult::Valid),
            InnerStandardChannelFactoryResponse::ValidShareWithAcknowledgement(
                last_sequence_number,
                new_submits_accepted_count,
                new_shares_sum,
            ) => Ok(ShareValidationResult::ValidWithAcknowledgement(
                last_sequence_number,
                new_submits_accepted_count,
                new_shares_sum,
            )),
            InnerStandardChannelFactoryResponse::BlockFound(template_id, coinbase) => {
                Ok(ShareValidationResult::BlockFound(template_id, coinbase))
            }
            InnerStandardChannelFactoryResponse::InvalidShare => {
                Err(StandardChannelFactoryError::InvalidShare)
            }
            InnerStandardChannelFactoryResponse::StaleShare => {
                Err(StandardChannelFactoryError::StaleShare)
            }
            InnerStandardChannelFactoryResponse::InvalidJobId => {
                Err(StandardChannelFactoryError::InvalidJobId)
            }
            InnerStandardChannelFactoryResponse::ShareDoesNotMeetTarget => {
                Err(StandardChannelFactoryError::ShareDoesNotMeetTarget)
            }
            InnerStandardChannelFactoryResponse::DuplicateShare => {
                Err(StandardChannelFactoryError::DuplicateShare)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the share accounting for a given channel.
    pub fn get_share_accounting(
        &self,
        channel_id: u32,
    ) -> Result<ShareAccounting, StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::GetShareAccounting(channel_id),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::ShareAccounting(share_accounting) => {
                Ok(share_accounting)
            }
            InnerStandardChannelFactoryResponse::StandardChannelIdNotFound => {
                Err(StandardChannelFactoryError::StandardChannelNotFound)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Sets the `extranonce_prefix` for a given standard channel, effectively overwriting the one
    /// originally assigned by the `ExtranoncePrefixFactoryStandard`.
    ///
    /// The length of the `extranonce_prefix` must match the one originally assigned by the
    /// `ExtranoncePrefixFactoryStandard`.
    ///
    /// There's no validation for potential collisions with other standard channels, so it's up to
    /// the caller to ensure that the new `extranonce_prefix` is unique.
    pub fn set_extranonce_prefix(
        &self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<(), StandardChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerStandardChannelFactoryMessage::SetExtranoncePrefix(channel_id, extranonce_prefix),
        )?;

        match response {
            InnerStandardChannelFactoryResponse::SetExtranoncePrefix => Ok(()),
            InnerStandardChannelFactoryResponse::StandardChannelIdNotFound => {
                Err(StandardChannelFactoryError::StandardChannelNotFound)
            }
            InnerStandardChannelFactoryResponse::ExtranoncePrefixLengthMismatch => {
                Err(StandardChannelFactoryError::ExtranoncePrefixLengthMismatch)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }

    pub fn get_chain_tip(&self) -> Result<ChainTip, StandardChannelFactoryError> {
        let response = self.inner_factory_io(InnerStandardChannelFactoryMessage::GetChainTip)?;

        match response {
            InnerStandardChannelFactoryResponse::ChainTip(chain_tip) => Ok(chain_tip),
            InnerStandardChannelFactoryResponse::ChainTipNotSet => {
                Err(StandardChannelFactoryError::ChainTipNotSet)
            }
            _ => Err(StandardChannelFactoryError::UnexpectedResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_management::id::ChannelIdFactory;
    use binary_sv2::u256_from_int;
    use mining_sv2::MAX_EXTRANONCE_LEN;
    use std::convert::TryInto;
    use stratum_common::bitcoin::{
        secp256k1::Secp256k1, Amount, Network, PrivateKey, PublicKey, ScriptBuf,
    };

    // Function to create a new public key for tests
    fn new_pub_key() -> PublicKey {
        const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
        const NETWORK: Network = Network::Testnet;

        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();

        PublicKey::from_private_key(&secp, &priv_k)
    }

    #[tokio::test]
    async fn test_update_standard_channel() {
        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        // Create a test channel
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let initial_hashrate = 100.0; // Start with a lower hashrate

        let (channel_id, initial_target, _) = factory
            .new_standard_channel(
                "test_user".to_string(),
                initial_hashrate,
                max_target.clone(),
                group_channel_id,
            )
            .expect("Failed to create test channel");

        // Get the initial channel state
        let initial_channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get initial channel");

        assert_eq!(initial_channel.get_nominal_hashrate(), initial_hashrate);

        // Update the channel with new parameters - much higher hashrate
        let new_hashrate = 1_000_000.0;
        let new_max_target = max_target.clone();

        let updated_target = factory
            .update_standard_channel(channel_id, new_hashrate, new_max_target)
            .expect("Failed to update channel");

        // Verify the channel was updated
        let updated_channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get updated channel");

        // Check that the hashrate was updated
        assert_eq!(updated_channel.get_nominal_hashrate(), new_hashrate);

        // Check that the target was updated
        assert_ne!(
            initial_target, updated_target,
            "Target should be different after update"
        );

        // Clean up
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_on_new_template_non_future() {
        const SATS_AVAILABLE_IN_TEMPLATE: u64 = 625_000_000_000;

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(0..0, 0..8, 8..32, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        // Create a test standard channel
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let initial_hashrate = 100.0; // Start with a lower hashrate

        let (channel_id, _, _) = factory
            .new_standard_channel(
                "test_user".to_string(),
                initial_hashrate,
                max_target.clone(),
                group_channel_id,
            )
            .expect("Failed to create test channel");

        // Create a valid NewTemplate with dummy values
        let coinbase_prefix: binary_sv2::B0255 = vec![5, 0, 1, 2, 3, 4].try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::B064K = vec![0; 32].try_into().unwrap();
        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: false, // non-future template
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix: coinbase_prefix.clone(),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: coinbase_tx_outputs.clone(),
            coinbase_tx_locktime: 0,
            merkle_path: merkle_path.clone(),
        };

        // Create valid coinbase reward outputs that sum to the value in the template
        let pub_key = new_pub_key();
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        // trigger StandardChannelFactoryError::ChainTipNotSet edge case
        let result =
            factory.process_new_template(new_template.clone(), coinbase_reward_outputs.clone());

        assert!(matches!(
            result.unwrap_err(),
            StandardChannelFactoryError::ChainTipNotSet
        ));

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true, // future template
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix: coinbase_prefix.clone(),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: coinbase_tx_outputs.clone(),
            coinbase_tx_locktime: 0,
            merkle_path: merkle_path.clone(),
        };

        // set a future template for activation of chain tip
        factory
            .process_new_template(new_template.clone(), coinbase_reward_outputs.clone())
            .expect("Failed to process new template");

        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: u256_from_int(0x00000000_u32),
            n_bits: 0x1d00ffff,
            header_timestamp: 0x60000000,
            target: u256_from_int(0xFFFFFFFFFFFFFFFF_u64),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let new_template = NewTemplate {
            template_id: 1,
            future_template: false, // non-future template
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix,
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs,
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        // try to process non-future template again
        // now with a valid chain tip
        let result =
            factory.process_new_template(new_template.clone(), coinbase_reward_outputs.clone());

        assert!(result.is_ok(), "on_new_template should succeed");

        // get the group channel and verify it has processed the template
        let group_channel = factory
            .get_group_channel(group_channel_id)
            .expect("Failed to get group channel");

        assert!(group_channel.get_active_job().is_some());

        // get the standard channel and verify it has processed the template
        let standard_channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get standard channel");

        assert!(standard_channel.get_active_job().is_some());
    }

    #[tokio::test]
    async fn test_on_new_template_future() {
        const SATS_AVAILABLE_IN_TEMPLATE: u64 = 625_000_000_000;

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        // Create a test standard channel
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let initial_hashrate = 100.0; // Start with a lower hashrate

        let (channel_id, _, _) = factory
            .new_standard_channel(
                "test_user".to_string(),
                initial_hashrate,
                max_target.clone(),
                group_channel_id,
            )
            .expect("Failed to create test channel");

        // Create a valid NewTemplate with dummy values
        let coinbase_prefix: binary_sv2::B0255 = vec![5, 0, 1, 2, 3, 4].try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::B064K = vec![0; 32].try_into().unwrap();
        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix,
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: SATS_AVAILABLE_IN_TEMPLATE,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs,
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        // Create valid coinbase reward outputs that sum to the value in the template
        let pub_key = new_pub_key();

        // Test with invalid coinbase reward outputs (sum is above the quantity available in
        // template)
        let invalid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE + 1),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        let result = factory.process_new_template(
            new_template.clone(),
            invalid_coinbase_reward_outputs.clone(),
        );

        assert!(matches!(
            result.unwrap_err(),
            StandardChannelFactoryError::InvalidCoinbaseRewardOutputs
        ));

        // Test with valid coinbase reward outputs (sum is equal to the quantity available in
        // template)
        let valid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        let result = factory
            .process_new_template(new_template.clone(), valid_coinbase_reward_outputs.clone());

        assert!(
            result.is_ok(),
            "on_new_template should succeed with valid coinbase reward outputs"
        );

        // get the group channel and verify it has processed the template
        let group_channel = factory
            .get_group_channel(group_channel_id)
            .expect("Failed to get group channel");

        assert!(group_channel.get_future_jobs().len() == 1);

        // get the standard channel and verify it has processed the template
        let standard_channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get standard channel");

        assert!(standard_channel.get_future_jobs().len() == 1);

        // shutdown the factory
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_block_found() {
        let pool_signature = "Stratum V2 SRI Pool".as_bytes().to_vec();
        let pool_signature_len = pool_signature.len();
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1_end = pool_signature_len + 8;
        let range_1 = std::ops::Range {
            start: 0,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        let user_identity = "test_user".to_string();
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        let (channel_id, _, _) = factory
            .new_standard_channel(
                user_identity,
                nominal_hashrate,
                max_target,
                group_channel_id,
            )
            .expect("Failed to create test channel");

        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(5000000000),
            script_pubkey: script,
        }];

        // process the new template
        factory
            .process_new_template(new_template, coinbase_reward_outputs)
            .expect("Failed to process new template");

        // process a chain tip
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                251, 175, 106, 40, 35, 87, 122, 90, 58, 51, 78, 32, 202, 236, 228, 36, 154, 174,
                206, 144, 147, 195, 21, 224, 195, 103, 214, 189, 51, 190, 24, 98,
            ]
            .into(),
            n_bits: 545259519,
            header_timestamp: 1745596910,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 255, 255, 127,
            ]
            .into(),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let share_valid_block = SubmitSharesStandard {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
        };

        let result = factory.validate_share(share_valid_block);

        assert!(matches!(
            result,
            Ok(ShareValidationResult::BlockFound(Some(1), _))
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_does_not_meet_target() {
        let pool_signature = "Stratum V2 SRI Pool".as_bytes().to_vec();
        let pool_signature_len = pool_signature.len();
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1_end = pool_signature_len + 8;
        let range_1 = std::ops::Range {
            start: 0,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        let user_identity = "test_user".to_string();
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _) = factory
            .new_standard_channel(
                user_identity,
                nominal_hashrate,
                max_target,
                group_channel_id,
            )
            .expect("Failed to create test channel");

        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(5000000000),
            script_pubkey: script,
        }];

        // process the new template
        factory
            .process_new_template(new_template, coinbase_reward_outputs)
            .expect("Failed to process new template");

        // process a chain tip
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                154, 124, 239, 231, 221, 122, 160, 173, 164, 175, 87, 33, 74, 214, 191, 107, 73,
                34, 0, 162, 227, 16, 44, 40, 33, 73, 0, 0, 0, 0, 0, 0,
            ]
            .into(),
            n_bits: 453040064,
            header_timestamp: 1745596910,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 215,
                0, 0, 0, 0, 0, 0,
            ]
            .into(),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let share_low_diff = SubmitSharesStandard {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
        };

        let result = factory.validate_share(share_low_diff);

        assert!(matches!(
            result.unwrap_err(),
            StandardChannelFactoryError::ShareDoesNotMeetTarget
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_valid_share() {
        let pool_signature = "Stratum V2 SRI Pool".as_bytes().to_vec();
        let pool_signature_len = pool_signature.len();
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1_end = pool_signature_len + 8;
        let range_1 = std::ops::Range {
            start: 0,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            2,   // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        let user_identity = "test_user".to_string();
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _) = factory
            .new_standard_channel(
                user_identity,
                nominal_hashrate,
                max_target,
                group_channel_id,
            )
            .expect("Failed to create test channel");

        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![3, 172, 173, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(5000000000),
            script_pubkey: script,
        }];

        // process the new template
        factory
            .process_new_template(new_template, coinbase_reward_outputs)
            .expect("Failed to process new template");

        // process a chain tip
        // network tarkget is: 000000000000d7c0000000000000000000000000000000000000000000000000
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                23, 205, 72, 134, 153, 86, 220, 153, 224, 28, 216, 146, 228, 120, 227, 157, 213,
                99, 160, 163, 128, 59, 139, 190, 158, 62, 0, 0, 0, 0, 0, 0,
            ]
            .into(),
            n_bits: 453040064,
            header_timestamp: 1745611105,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 215,
                0, 0, 0, 0, 0, 0,
            ]
            .into(),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let valid_share = SubmitSharesStandard {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 42354,
            ntime: 1745611105,
            version: 536870912,
        };

        let result = factory.validate_share(valid_share);

        assert!(matches!(result, Ok(ShareValidationResult::Valid)));

        let share_accounting = factory
            .get_share_accounting(channel_id)
            .expect("Failed to get share accounting");

        assert_eq!(share_accounting.get_shares_accepted(), 1);
        assert_eq!(share_accounting.get_last_share_sequence_number(), 1);
        assert!(!share_accounting.should_acknowledge());

        let valid_share = SubmitSharesStandard {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 95078,
            ntime: 1745611105,
            version: 536870912,
        };

        let result = factory.validate_share(valid_share);

        let share_accounting = factory
            .get_share_accounting(channel_id)
            .expect("Failed to get share accounting");

        assert_eq!(share_accounting.get_shares_accepted(), 2);
        assert_eq!(share_accounting.get_last_share_sequence_number(), 2);
        assert!(share_accounting.should_acknowledge());

        assert!(matches!(
            result,
            Ok(ShareValidationResult::ValidWithAcknowledgement(_, _, _))
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_repeated_share() {
        let pool_signature = "Stratum V2 SRI Pool".as_bytes().to_vec();
        let pool_signature_len = pool_signature.len();
        let range_0 = std::ops::Range { start: 0, end: 0 };
        let range_1_end = pool_signature_len + 8;
        let range_1 = std::ops::Range {
            start: 0,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        let user_identity = "test_user".to_string();
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _) = factory
            .new_standard_channel(
                user_identity,
                nominal_hashrate,
                max_target,
                group_channel_id,
            )
            .expect("Failed to create test channel");

        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![3, 172, 173, 0, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        let pubkey_hash = [
            235, 225, 183, 220, 194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194,
            8, 252,
        ];
        let mut script_bytes = vec![0]; // SegWit version 0
        script_bytes.push(20); // Push 20 bytes (length of pubkey hash)
        script_bytes.extend_from_slice(&pubkey_hash);
        let script = ScriptBuf::from(script_bytes);
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(5000000000),
            script_pubkey: script,
        }];

        // process the new template
        factory
            .process_new_template(new_template, coinbase_reward_outputs)
            .expect("Failed to process new template");

        // process a chain tip
        // network tarkget is: 000000000000d7c0000000000000000000000000000000000000000000000000
        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: [
                23, 205, 72, 134, 153, 86, 220, 153, 224, 28, 216, 146, 228, 120, 227, 157, 213,
                99, 160, 163, 128, 59, 139, 190, 158, 62, 0, 0, 0, 0, 0, 0,
            ]
            .into(),
            n_bits: 453040064,
            header_timestamp: 1745611105,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 215,
                0, 0, 0, 0, 0, 0,
            ]
            .into(),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let valid_share = SubmitSharesStandard {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 42354,
            ntime: 1745611105,
            version: 536870912,
        };

        let result = factory.validate_share(valid_share);

        assert!(matches!(result, Ok(ShareValidationResult::Valid)));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let valid_share = SubmitSharesStandard {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 42354,
            ntime: 1745611105,
            version: 536870912,
        };

        let result = factory.validate_share(valid_share);

        assert!(matches!(
            result,
            Err(StandardChannelFactoryError::DuplicateShare)
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_set_extranonce_prefix() {
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryStandard::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = StandardChannelFactory::new(
            extranonce_prefix_factory,
            1.0, // expected_share_per_minute_per_channel
            10,  // share_batch_size
            channel_id_factory,
        );

        // Create a test channel first
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let initial_hashrate = 100.0;

        // Create a group channel first, as all standard channels must be part of a group channel
        let group_channel_id = factory
            .new_group_channel()
            .expect("Failed to create group channel");

        let (channel_id, _, _) = factory
            .new_standard_channel(
                "test_user".to_string(),
                initial_hashrate,
                max_target,
                group_channel_id,
            )
            .expect("Failed to create test channel");

        let channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get standard channel");

        let initial_extranonce_prefix = channel.get_extranonce_prefix();
        let initial_extranonce_prefix_len = initial_extranonce_prefix.len();

        let new_extranonce_prefix = vec![1u8; initial_extranonce_prefix_len];

        let result = factory.set_extranonce_prefix(channel_id, new_extranonce_prefix.clone());

        assert!(result.is_ok());

        let channel = factory
            .get_standard_channel(channel_id)
            .expect("Failed to get standard channel");

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix.clone(), new_extranonce_prefix);

        let new_extranonce_prefix_bad_length = vec![1u8; initial_extranonce_prefix_len - 1];

        let result = factory.set_extranonce_prefix(channel_id, new_extranonce_prefix_bad_length);

        assert!(matches!(
            result,
            Err(StandardChannelFactoryError::ExtranoncePrefixLengthMismatch)
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }
}
