//! A Factory for creating and managing [`ExtendedChannel`] instances.

pub mod error;
mod inner;
mod message;
mod response;

use crate::extranonce_prefix_management::extended::ExtranoncePrefixFactoryExtended;
use binary_sv2::U256;

use crate::{
    channel_management::{
        extended::{
            channel::ExtendedChannel,
            factory::{
                error::ExtendedChannelFactoryError,
                inner::{InnerExtendedChannelFactory, InnerExtendedChannelFactoryIo},
                message::InnerExtendedChannelFactoryMessage,
                response::InnerExtendedChannelFactoryResponse,
            },
        },
        id::ChannelIdFactory,
        share_accounting::{ShareAccounting, ShareValidationResult},
    },
    job_management::chain_tip::ChainTip,
};
use mining_sv2::{SetCustomMiningJob, SubmitSharesExtended};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
};
use stratum_common::bitcoin::{consensus::Decodable, transaction::TxOut};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash};

/// A Factory for creating and managing [`ExtendedChannel`] instances under the [Actor Model](https://en.wikipedia.org/wiki/Actor_model).
///
/// Only suitable for mining servers, not clients.
///
/// It spawns a background [`tokio::task::spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) task that handles all state-changes in a
/// concurrency-safe manner. Please note that this is a blocking call spawned on tokio's blocking
/// scheduler. This means that an application should not create new instances of
/// `ExtendedChannelFactory` without bounds, as it would eventually create a deadlock on the tokio's
/// blocking scheduler.
///
/// Therefore, a Sv2 application should deploy one `ExtendedChannelFactory` for the entire
/// application, and avoid creating new instances for each new client connection.
///
/// Allows the user to manage channel states upon receipt of the following Sv2 messages:
/// - Mining Protocol:
///   - `OpenExtendedMiningChannel`: create new channel
///   - `UpdateChannel`: update channel nominal hashrate and target
///   - `CloseChannel`: remove channel
///   - `SetCustomMiningJob`: validate and add new custom mining job
///   - `SubmitSharesExtended`: validate shares
/// - Template Distribution Protocol:
///   - `NewTemplate`: update channel jobs
///   - `SetNewPrevHash`: update chain tip for share validation, activate future jobs
///
/// The user is responsible for creating the appropriate Sv2 messages to be sent to the client upon
/// state changes triggered by methods calls. Methods return all necessary information for creating
/// those messages.
///
/// If the `shutdown` method is not called, the caller is under risk of leaving behind a zombie
/// task.
#[derive(Debug, Clone)]
pub struct ExtendedChannelFactory {
    message_sender_into_inner_factory: mpsc::Sender<InnerExtendedChannelFactoryIo>,
}

impl ExtendedChannelFactory {
    /// Creates a new factory and starts its background thread for managing the internal state under
    /// the Actor Model.
    ///
    /// This is the main entry point for creating and starting an `ExtendedChannelFactory`.
    pub fn new(
        extranonce_prefix_factory: ExtranoncePrefixFactoryExtended,
        expected_share_per_minute_per_channel: f32,
        version_rolling_allowed: bool,
        share_batch_size: usize,
        channel_id_factory: ChannelIdFactory,
    ) -> Self {
        // Create mpsc for communication with inner Actor
        let (message_sender, message_receiver) = mpsc::channel();

        let rollable_extranonce_size = extranonce_prefix_factory.get_rollable_extranonce_size();

        // Create inner factory instance that will run in the background task
        // effectively implementing the Actor Model
        let inner_factory = InnerExtendedChannelFactory::new(
            message_receiver,
            extranonce_prefix_factory,
            expected_share_per_minute_per_channel,
            rollable_extranonce_size,
            version_rolling_allowed,
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
        message: InnerExtendedChannelFactoryMessage<'static>,
    ) -> Result<InnerExtendedChannelFactoryResponse<'static>, ExtendedChannelFactoryError> {
        // Create an ephemeral mspc channel for the response
        let (response_sender, response_receiver) = mpsc::channel();

        // Send the message to the inner factory
        self.message_sender_into_inner_factory
            .send(InnerExtendedChannelFactoryIo::new(message, response_sender))
            .map_err(|_| ExtendedChannelFactoryError::MessageSenderError)?;

        // Wait for the response
        response_receiver
            .recv()
            .map_err(|_| ExtendedChannelFactoryError::ResponseReceiverError)
    }

    /// Shuts down the factory runner task.
    pub fn shutdown(&self) -> Result<(), ExtendedChannelFactoryError> {
        // Send the shutdown message and wait for the response
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::Shutdown)?;

        match response {
            InnerExtendedChannelFactoryResponse::Shutdown => Ok(()),
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Creates a new channel. To be called upon receipt of a `OpenExtendedMiningChannel` Sv2
    /// message.
    ///
    /// On success, it returns:
    /// - The channel id of the new channel
    /// - The target of the new channel
    /// - The rollable extranonce size of the new channel
    /// - The extranonce prefix of the new channel
    ///
    /// These values should be used by the caller to craft a OpenExtendedMiningChannel.Success
    /// message to be sent to the client
    ///
    /// On error, it returns a `ExtendedChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `RequestedMaxTargetTooHigh` (which should be used by the caller to craft a
    ///   `OpenMiningChannel.Error` message with `max_target_out_of_range` as `error_code)
    ///
    /// Other error variants should be treated as some internal error, but not necessarily something
    /// to be reported via `OpenMiningChannel.Error`.
    pub fn new_channel(
        &self,
        user_identity: String,
        nominal_hashrate: f32,
        min_extranonce_size: usize,
        max_target: U256<'static>,
    ) -> Result<(u32, U256<'static>, u16, Vec<u8>), ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::NewChannel(
            user_identity,
            nominal_hashrate,
            min_extranonce_size,
            max_target,
        ))?;

        match response {
            InnerExtendedChannelFactoryResponse::NewChannelCreated(
                channel_id,
                target,
                rollable_extranonce_size,
                extranonce_prefix,
            ) => {
                // caller should use this for OpenExtendedMiningChannel.Success
                Ok((
                    channel_id,
                    target,
                    rollable_extranonce_size,
                    extranonce_prefix,
                ))
            }
            InnerExtendedChannelFactoryResponse::RequestedMaxTargetOutOfRange => {
                // caller should match against this error variant
                // in order to properly craft a `OpenMiningChannel.Error` message
                // with max-target-out-of-range as error_code
                Err(ExtendedChannelFactoryError::RequestedMaxTargetOutOfRange)
            }
            InnerExtendedChannelFactoryResponse::InvalidNominalHashrate => {
                Err(ExtendedChannelFactoryError::InvalidNominalHashrate)
            }
            InnerExtendedChannelFactoryResponse::FailedToGenerateNextExtranoncePrefixExtended(
                e,
            ) => Err(ExtendedChannelFactoryError::FailedToGenerateNextExtranoncePrefixExtended(e)),
            InnerExtendedChannelFactoryResponse::FailedToGenerateNextChannelId(e) => Err(
                ExtendedChannelFactoryError::FailedToGenerateNextChannelId(e),
            ),
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of the channel with the specified id, if it exists.
    ///
    /// Any modifications made to the returned channel will not affect the original channel
    /// stored in the factory, as it returns a completely independent copy.
    pub fn get_channel(
        &self,
        channel_id: u32,
    ) -> Result<ExtendedChannel, ExtendedChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerExtendedChannelFactoryMessage::GetChannel(channel_id))?;

        match response {
            InnerExtendedChannelFactoryResponse::Channel(channel) => Ok(channel),
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the number of channels currently in the factory.
    pub fn get_channel_count(&self) -> Result<u32, ExtendedChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerExtendedChannelFactoryMessage::GetChannelCount)?;

        match response {
            InnerExtendedChannelFactoryResponse::ChannelCount(channel_count) => Ok(channel_count),
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns a clone of all channels currently in the factory.
    ///
    /// Any modifications made to the returned channels will not affect the original channels
    /// stored in the factory, as it returns a completely independent copy.
    pub fn get_all_channels(
        &self,
    ) -> Result<HashMap<u32, ExtendedChannel>, ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::GetAllChannels)?;

        match response {
            InnerExtendedChannelFactoryResponse::AllChannels(channels) => Ok(channels),
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Removes the channel with the specified id, if it exists.
    pub fn remove_channel(&self, channel_id: u32) -> Result<(), ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::RemoveChannel(
            channel_id,
        ))?;

        match response {
            InnerExtendedChannelFactoryResponse::ChannelRemoved => Ok(()),
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Updates the channel with the specified id, if it exists.
    ///
    /// On success, it returns the new max target of the channel. This shall be used by the caller
    /// to craft a `SetTarget` Sv2 message to be sent to the client across the wire.
    ///
    /// On error, it returns a `ExtendedChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `RequestedMaxTargetTooHigh` (which should be used by the caller to craft a
    ///   `UpdateChannel.Error` message with `max_target_out_of_range` as `error_code`)
    pub fn update_channel(
        &self,
        channel_id: u32,
        nominal_hashrate: f32,
        max_target: U256<'static>,
    ) -> Result<U256<'static>, ExtendedChannelFactoryError> {
        // Send the update channel message and wait for the response
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::UpdateChannel(
            channel_id,
            nominal_hashrate,
            max_target,
        ))?;

        match response {
            InnerExtendedChannelFactoryResponse::ChannelUpdated(new_max_target) => {
                Ok(new_max_target)
            }
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            InnerExtendedChannelFactoryResponse::RequestedMaxTargetOutOfRange => {
                Err(ExtendedChannelFactoryError::RequestedMaxTargetOutOfRange)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Processes a `SetCustomMiningJob` Sv2 message.
    ///
    /// It effectively updates some specific channel state such that it's active job is the one
    /// specified in the `SetCustomMiningJob` message, after validating the messages for
    /// edge cases.
    ///
    /// On success, returns the `job_id` of the new custom mining job. This shall be used by the
    /// caller to craft a `SetCustomMiningJob.Success` Sv2 message to be sent to the client across
    /// the wire.
    ///
    /// On error, it returns a `ExtendedChannelFactoryError`, where the following variant should be
    /// specially treated by the caller:
    /// - `CustomMiningJobBadChannelId` (which should be used by the caller to craft a
    ///   `SetCustomMiningJob.Error` message with `error_code = invalid-channel-id`)
    /// - `CustomMiningJobBadPrevHash` (which should be used by the caller to craft a
    ///   `SetCustomMiningJob.Error` message with `error_code = invalid-job-param-value-prev-hash`)
    /// - `CustomMiningJobBadNbits` (which should be used by the caller to craft a
    ///   `SetCustomMiningJob.Error` message with `error_code = invalid-job-param-value-nbits`)
    /// - `CustomMiningJobBadNtime` (which should be used by the caller to craft a
    ///   `SetCustomMiningJob.Error` message with `error_code`)
    /// - `CustomMiningJobBadCoinbaseRewardOutputs` (which should be used by the caller to craft a
    ///   `SetCustomMiningJob.Error` message with `error_code =
    ///   invalid-job-param-value-coinbase-tx-outputs`)
    ///
    /// Note that this method does not do any kind of validation of the `mining_job_token` field,
    /// which is assumed to have already been validated by the caller.
    ///
    /// Note that this simply updates the state of the channel. After calling this method,
    /// the caller is still responsible crafting the appropriate Sv2 messages to be sent to the
    /// clients across the wire (e.g.: `SetCustomMiningJob.Success`).
    pub fn process_set_custom_mining_job(
        &self,
        set_custom_mining_job: SetCustomMiningJob<'static>,
        expected_coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<u32, ExtendedChannelFactoryError> {
        // --------------------------------------------------------------------------------------------
        // first, we validate the coinbase reward outputs on this custom mining job
        // we do this even before sending the message to the inner factory, as it's completely
        // independent from its current state
        // --------------------------------------------------------------------------------------------
        let mut coinbase_reward_outputs: Vec<TxOut> = vec![];
        let serialized_outputs = set_custom_mining_job
            .coinbase_tx_outputs
            .inner_as_ref()
            .to_vec();

        // The serialized outputs are in Bitcoin consensus format
        // We need to parse them one by one, keeping track of cursor position
        let mut cursor = 0;
        let mut txouts = &serialized_outputs[cursor..];

        // Iteratively decode each TxOut until we can't decode any more
        while let Ok(out) = TxOut::consensus_decode(&mut txouts) {
            // Calculate the size of this TxOut based on its script_pubkey length
            // 8 bytes for value + variable bytes for script_pubkey length
            // For small scripts (0-252 bytes): 1 byte length prefix
            // For medium scripts (253-1000000 bytes): 3 byte length prefix (1 marker + 2 byte
            // length)
            let len = match out.script_pubkey.len() {
                a @ 0..=252 => 8 + 1 + a,       // 8 (value) + 1 (compact size) + script_len
                a @ 253..=1000000 => 8 + 3 + a, // 8 (value) + 3 (compact size) + script_len
                _ => break,                     // Unreasonably large script, likely an error
            };

            // Move the cursor forward by the size of this TxOut
            cursor += len;
            coinbase_reward_outputs.push(out);
        }

        let mut expected_script_pubkey_set = HashSet::new();
        for output in expected_coinbase_reward_outputs {
            expected_script_pubkey_set.insert(output.script_pubkey);
        }

        let mut proposed_script_pubkey_set = HashSet::new();
        for output in coinbase_reward_outputs {
            proposed_script_pubkey_set.insert(output.script_pubkey);
        }

        // check that all script_pubkeys in expected_script_pubkey_set are present in
        // proposed_script_pubkey_set
        for script_pubkey in expected_script_pubkey_set {
            if !proposed_script_pubkey_set.contains(&script_pubkey) {
                return Err(ExtendedChannelFactoryError::CustomMiningJobBadCoinbaseRewardOutputs);
            }
        }

        // --------------------------------------------------------------------------------------------
        // end of validation of coinbase reward outputs
        // --------------------------------------------------------------------------------------------

        // Send the process set custom mining job message and wait for the response
        let response = self.inner_factory_io(
            InnerExtendedChannelFactoryMessage::ProcessSetCustomMiningJob(set_custom_mining_job),
        )?;

        match response {
            InnerExtendedChannelFactoryResponse::ProcessedSetCustomMiningJob(job_id) => Ok(job_id),
            InnerExtendedChannelFactoryResponse::CustomMiningJobBadChannelId => {
                Err(ExtendedChannelFactoryError::CustomMiningJobBadChannelId)
            }
            InnerExtendedChannelFactoryResponse::CustomMiningJobBadPrevHash => {
                Err(ExtendedChannelFactoryError::CustomMiningJobBadPrevHash)
            }
            InnerExtendedChannelFactoryResponse::CustomMiningJobBadNbits => {
                Err(ExtendedChannelFactoryError::CustomMiningJobBadNbits)
            }
            InnerExtendedChannelFactoryResponse::CustomMiningJobBadNtime => {
                Err(ExtendedChannelFactoryError::CustomMiningJobBadNtime)
            }
            InnerExtendedChannelFactoryResponse::ProcessSetCustomMiningJobChannelError(e) => {
                Err(ExtendedChannelFactoryError::ProcessSetCustomMiningJobChannelError(e))
            }
            InnerExtendedChannelFactoryResponse::ChainTipNotSet => {
                Err(ExtendedChannelFactoryError::ChainTipNotSet)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
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
    /// Note that this simply updates the state of the channels. After calling this method,
    /// the caller is still responsible for querying the state of each channel in order to craft
    /// the appropriate Sv2 messages to be sent to the clients across the wire (e.g.:
    /// `NewExtendedMiningJob`).
    pub fn process_new_template(
        &self,
        template: NewTemplate<'static>,
        coinbase_reward_outputs: Vec<TxOut>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        let response =
            self.inner_factory_io(InnerExtendedChannelFactoryMessage::ProcessNewTemplate(
                template,
                coinbase_reward_outputs,
            ))?;

        match response {
            InnerExtendedChannelFactoryResponse::ProcessedNewTemplate => Ok(()),
            InnerExtendedChannelFactoryResponse::ChainTipNotSet => {
                Err(ExtendedChannelFactoryError::ChainTipNotSet)
            }
            InnerExtendedChannelFactoryResponse::InvalidCoinbaseRewardOutputs => {
                Err(ExtendedChannelFactoryError::InvalidCoinbaseRewardOutputs)
            }
            InnerExtendedChannelFactoryResponse::ProcessNewTemplateChannelError(e) => Err(
                ExtendedChannelFactoryError::ProcessNewTemplateChannelError(e),
            ),
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
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
    ) -> Result<(), ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerExtendedChannelFactoryMessage::ProcessSetNewPrevHash(set_new_prev_hash),
        )?;

        match response {
            InnerExtendedChannelFactoryResponse::ProcessedSetNewPrevHash => Ok(()),
            InnerExtendedChannelFactoryResponse::TemplateNotFound => {
                Err(ExtendedChannelFactoryError::TemplateNotFound)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Validates a share.
    ///
    /// To be called by mining servers upon receipt of a `SubmitSharesExtended` message.
    ///
    /// It updates the state of the channel with the appropriate share accounting.
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
    /// On error, it returns a `ExtendedChannelFactoryError`, where the following variants should be
    /// specially treated by the caller:
    /// - `InvalidShare`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   invalid-share`.
    /// - `StaleShare`: This should lead to a `SubmitShares.Error` with `error_code = stale-share`.
    /// - `InvalidJobId`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   invalid-job-id`.
    /// - `ShareDoesNotMeetTarget`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   difficulty-too-low`.
    /// - `DuplicateShare`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   duplicate-share`.
    /// - `VersionRollingNotAllowed`: This should lead to a `SubmitShares.Error` with `error_code =
    ///   version-rolling-not-allowed`.
    pub fn validate_share(
        &self,
        submit_shares_extended: SubmitSharesExtended<'static>,
    ) -> Result<ShareValidationResult, ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::ValidateShare(
            submit_shares_extended,
        ))?;

        match response {
            InnerExtendedChannelFactoryResponse::ValidShare => Ok(ShareValidationResult::Valid),
            InnerExtendedChannelFactoryResponse::ValidShareWithAcknowledgement(
                last_sequence_number,
                new_submits_accepted_count,
                new_shares_sum,
            ) => Ok(ShareValidationResult::ValidWithAcknowledgement(
                last_sequence_number,
                new_submits_accepted_count,
                new_shares_sum,
            )),
            InnerExtendedChannelFactoryResponse::BlockFound(template_id, coinbase) => {
                Ok(ShareValidationResult::BlockFound(template_id, coinbase))
            }
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            InnerExtendedChannelFactoryResponse::InvalidShare => {
                Err(ExtendedChannelFactoryError::InvalidShare)
            }
            InnerExtendedChannelFactoryResponse::StaleShare => {
                Err(ExtendedChannelFactoryError::StaleShare)
            }
            InnerExtendedChannelFactoryResponse::InvalidJobId => {
                Err(ExtendedChannelFactoryError::InvalidJobId)
            }
            InnerExtendedChannelFactoryResponse::ShareDoesNotMeetTarget => {
                Err(ExtendedChannelFactoryError::ShareDoesNotMeetTarget)
            }
            InnerExtendedChannelFactoryResponse::VersionRollingNotAllowed => {
                Err(ExtendedChannelFactoryError::VersionRollingNotAllowed)
            }
            InnerExtendedChannelFactoryResponse::DuplicateShare => {
                Err(ExtendedChannelFactoryError::DuplicateShare)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Returns the share accounting for a given channel.
    pub fn get_share_accounting(
        &self,
        channel_id: u32,
    ) -> Result<ShareAccounting, ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerExtendedChannelFactoryMessage::GetShareAccounting(channel_id),
        )?;

        match response {
            InnerExtendedChannelFactoryResponse::ShareAccounting(share_accounting) => {
                Ok(share_accounting)
            }
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    /// Sets the `extranonce_prefix` for a given channel, effectively overwriting the one originally
    /// assigned by the `ExtranoncePrefixFactoryExtended`.
    ///
    /// The new `extranonce_prefix` must be of the same length as the one assigned by the
    /// `ExtranoncePrefixFactoryExtended`.
    ///
    /// There's no validation for potential collisions with other channels, so it's up to the caller
    /// to ensure that the new `extranonce_prefix` is unique.
    pub fn set_extranonce_prefix(
        &self,
        channel_id: u32,
        extranonce_prefix: Vec<u8>,
    ) -> Result<(), ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(
            InnerExtendedChannelFactoryMessage::SetExtranoncePrefix(channel_id, extranonce_prefix),
        )?;

        match response {
            InnerExtendedChannelFactoryResponse::SetExtranoncePrefix => Ok(()),
            InnerExtendedChannelFactoryResponse::ChannelNotFound => {
                Err(ExtendedChannelFactoryError::ChannelNotFound)
            }
            InnerExtendedChannelFactoryResponse::ExtranoncePrefixLengthMismatch => {
                Err(ExtendedChannelFactoryError::ExtranoncePrefixLengthMismatch)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }

    pub fn get_chain_tip(&self) -> Result<ChainTip, ExtendedChannelFactoryError> {
        let response = self.inner_factory_io(InnerExtendedChannelFactoryMessage::GetChainTip)?;

        match response {
            InnerExtendedChannelFactoryResponse::ChainTip(chain_tip) => Ok(chain_tip),
            InnerExtendedChannelFactoryResponse::ChainTipNotSet => {
                Err(ExtendedChannelFactoryError::ChainTipNotSet)
            }
            _ => Err(ExtendedChannelFactoryError::UnexpectedResponse),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_management::id::ChannelIdFactory;
    use binary_sv2::u256_from_int;
    use mining_sv2::MAX_EXTRANONCE_LEN;
    use std::{
        collections::HashSet,
        convert::TryInto,
        sync::{Arc, Mutex},
    };
    use stratum_common::bitcoin::{
        consensus::Encodable, secp256k1::Secp256k1, Amount, Network, PrivateKey, PublicKey,
        ScriptBuf,
    };
    use template_distribution_sv2::NewTemplate;
    use tokio::{sync::Barrier as AsyncBarrier, task};

    // Function to create a new public key for tests
    fn new_pub_key() -> PublicKey {
        const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
        const NETWORK: Network = Network::Testnet;

        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();

        PublicKey::from_private_key(&secp, &priv_k)
    }

    #[tokio::test]
    async fn test_concurrency_safety_of_extended_mining_channel_creation() {
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..16, 16..32, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        // Create factory with generous ranges to avoid extranonce collisions in testing
        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Make factory accessible from multiple tasks
        let factory = Arc::new(factory);

        // Concurrent tasks to create channels
        const NUM_TASKS: usize = 1_000;

        // Synchronize tasks to start at the same time
        let barrier = Arc::new(AsyncBarrier::new(NUM_TASKS));

        // Parameter that all tasks will use
        // 0xFFFFFFFFFFFFFFFF00000000000000000000000000000000000000000000000000000000
        // this is an extremely high target, for which even an extremely low hashrate will be
        // accepted
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);

        // Channel IDs obtained from each task
        let channel_ids = Arc::new(Mutex::new(Vec::with_capacity(NUM_TASKS)));

        // Create tasks
        let mut handles = Vec::with_capacity(NUM_TASKS);

        // Spawn multiple tasks that create channels concurrently
        for task_id in 0..NUM_TASKS {
            let factory_clone = factory.clone();
            let barrier_clone = barrier.clone();
            let max_target_clone = max_target.clone();
            let channel_ids_clone = channel_ids.clone();

            let handle = task::spawn(async move {
                // Create unique parameters for this task
                let user_identity = format!("async_miner_{}", task_id);
                let nominal_hashrate = 1.0 + task_id as f32; // extremely low hashrate, but unique for each task
                let min_extranonce_size = 1; // don't change this to avoid issues with the extranonce prefix

                // Wait for all tasks to reach this point
                barrier_clone.wait().await;

                // Create channels concurrently
                let (channel_id, _target, rollable_extranonce_size, extranonce_prefix) =
                    factory_clone
                        .new_channel(
                            user_identity.clone(),
                            nominal_hashrate,
                            min_extranonce_size,
                            max_target_clone,
                        )
                        .unwrap_or_else(|_| panic!("Task {} failed to create channel", task_id));

                // Store the channel_id for verification
                let mut ids = channel_ids_clone.lock().unwrap();
                ids.push(channel_id);

                println!(
                    "Task {} successfully created channel {}",
                    task_id, channel_id
                );

                // Return the parameters for verification
                (
                    channel_id,
                    user_identity,
                    nominal_hashrate,
                    rollable_extranonce_size,
                    extranonce_prefix,
                )
            });

            handles.push(handle);
        }

        // Collect results from all tasks
        let mut results = Vec::with_capacity(NUM_TASKS);
        for handle in handles {
            results.push(handle.await.expect("Task panicked"));
        }

        // Verify all channels were created with unique IDs
        let ids = channel_ids.lock().unwrap();
        let unique_ids: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(unique_ids.len(), NUM_TASKS, "Channel IDs should be unique");

        // Now verify that we can retrieve all channels and their properties match
        for (
            channel_id,
            user_identity,
            nominal_hashrate,
            rollable_extranonce_size,
            extranonce_prefix,
        ) in results
        {
            let channel = factory
                .get_channel(channel_id)
                .unwrap_or_else(|_| panic!("Failed to get channel {}", channel_id));

            // Verify channel properties
            assert_eq!(channel.get_channel_id(), channel_id);
            assert_eq!(channel.get_user_identity(), &user_identity);
            assert_eq!(channel.get_nominal_hashrate(), nominal_hashrate);
            assert_eq!(
                channel.get_rollable_extranonce_size(),
                rollable_extranonce_size
            );
            assert_eq!(channel.get_extranonce_prefix(), &extranonce_prefix);

            println!("Channel {} successfully checked", channel_id);
        }

        // Verify channel count
        let channel_count = factory
            .get_channel_count()
            .expect("Failed to get channel count");
        assert_eq!(channel_count, NUM_TASKS as u32);

        // Verify all channels can be retrieved at once
        let all_channels = factory
            .get_all_channels()
            .expect("Failed to get all channels");
        assert_eq!(all_channels.len(), { NUM_TASKS });

        // Clean up
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_shared_extranonce_prefix_factory() {
        // Create a single ExtranoncePrefixFactoryExtended that will be shared
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        // Create two ExtendedChannelFactory instances that share the same
        // ExtranoncePrefixFactoryExtended
        let channel_factory_a = ExtendedChannelFactory::new(
            extranonce_prefix_factory.clone(),
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory.clone(),
        );

        let channel_factory_b = ExtendedChannelFactory::new(
            extranonce_prefix_factory.clone(),
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Define common parameters for channel creation
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;

        // Create a channel using the first factory
        let (channel_id_a, _, _, _) = channel_factory_a
            .new_channel(
                "user1".to_string(),
                1000.0, // nominal_hashrate
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel from factory a");

        // Create a channel using the second factory
        let (channel_id_b, _, _, _) = channel_factory_b
            .new_channel(
                "user2".to_string(),
                2000.0, // nominal_hashrate
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel from factory b");

        // Verify that the channels have the correct extranonce prefixes
        let channel_a = channel_factory_a
            .get_channel(channel_id_a)
            .expect("Failed to get channel a");

        let channel_b = channel_factory_b
            .get_channel(channel_id_b)
            .expect("Failed to get channel b");

        let extranonce_prefix_a = channel_a.get_extranonce_prefix();
        let extranonce_prefix_b = channel_b.get_extranonce_prefix();

        // Assert that the extranonce prefixes are unique, even though the channels were generated
        // by the same `ExtranoncePrefixFactoryExtended`
        assert_ne!(
            extranonce_prefix_a, extranonce_prefix_b,
            "Extranonce prefixes should be different"
        );
    }

    #[tokio::test]
    async fn test_remove_channel() {
        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create some test channels
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;

        // Create three channels
        let (channel_id_1, _, _, _) = factory
            .new_channel(
                "user1".to_string(),
                1000.0,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel 1");

        let (channel_id_2, _, _, _) = factory
            .new_channel(
                "user2".to_string(),
                2000.0,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel 2");

        let (channel_id_3, _, _, _) = factory
            .new_channel(
                "user3".to_string(),
                3000.0,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel 3");

        // Verify initial channel count
        let initial_count = factory
            .get_channel_count()
            .expect("Failed to get channel count");
        assert_eq!(initial_count, 3, "Should have 3 channels initially");

        // Remove the second channel
        factory
            .remove_channel(channel_id_2)
            .expect("Failed to remove channel 2");

        // Verify channel count decreased
        let count_after_removal = factory
            .get_channel_count()
            .expect("Failed to get channel count");
        assert_eq!(
            count_after_removal, 2,
            "Should have 2 channels after removal"
        );

        // Verify we can still get the remaining channels
        let _ = factory
            .get_channel(channel_id_1)
            .expect("Failed to get channel 1 after removal of channel 2");

        let _ = factory
            .get_channel(channel_id_3)
            .expect("Failed to get channel 3 after removal of channel 2");

        // Verify removed channel cannot be retrieved
        let result = factory.get_channel(channel_id_2);
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::ChannelNotFound
        ));

        // Remove remaining channels
        factory
            .remove_channel(channel_id_1)
            .expect("Failed to remove channel 1");

        factory
            .remove_channel(channel_id_3)
            .expect("Failed to remove channel 3");

        // Verify channel count is zero
        let final_count = factory
            .get_channel_count()
            .expect("Failed to get channel count");
        assert_eq!(final_count, 0, "Should have 0 channels after removing all");

        // Clean up
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_update_channel() {
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create a test channel
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;
        let initial_hashrate = 100.0; // Start with a lower hashrate

        let (channel_id, initial_target, _, _) = factory
            .new_channel(
                "test_user".to_string(),
                initial_hashrate,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel");

        // Get the initial channel state
        let initial_channel = factory
            .get_channel(channel_id)
            .expect("Failed to get initial channel");

        assert_eq!(initial_channel.get_nominal_hashrate(), initial_hashrate);

        // Update the channel with new parameters - much higher hashrate
        let new_hashrate = 1_000_000.0;
        let new_max_target = max_target.clone();

        let updated_target = factory
            .update_channel(channel_id, new_hashrate, new_max_target)
            .expect("Failed to update channel");

        // Verify the channel was updated
        let updated_channel = factory
            .get_channel(channel_id)
            .expect("Failed to get updated channel");

        // Check that the hashrate was updated
        assert_eq!(
            updated_channel.get_nominal_hashrate(),
            new_hashrate,
            "Nominal hashrate should be updated"
        );

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
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create a test channel first
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;
        let initial_hashrate = 100.0;

        let (channel_id, _, _, _) = factory
            .new_channel(
                "test_user".to_string(),
                initial_hashrate,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel");

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

        // trigger ExtendedChannelError::ChainTipNotSet edge case
        let result =
            factory.process_new_template(new_template.clone(), coinbase_reward_outputs.clone());
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::ChainTipNotSet
        ));

        // set a future template for activation of chain tip
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

        // Get the channel and verify it has processed the template
        let channel = factory
            .get_channel(channel_id)
            .expect("Failed to get channel");

        // Verify the channel has processed the template
        assert!(
            channel.get_active_job().is_some(),
            "Channel should have an active job"
        );

        // Clean up
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_on_new_template_future() {
        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create a test channel first
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;
        let initial_hashrate = 100.0;

        let (channel_id, _, _, _) = factory
            .new_channel(
                "test_user".to_string(),
                initial_hashrate,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel");

        // Create a valid NewTemplate with dummy values
        let coinbase_prefix: binary_sv2::B0255 = vec![5, 0, 1, 2, 3, 4].try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::B064K = vec![0; 32].try_into().unwrap();
        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        let new_template = NewTemplate {
            template_id: 1,
            future_template: true, // future template
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix,
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 625_000_000_000, // 6.25 BTC
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs,
            coinbase_tx_locktime: 0,
            merkle_path,
        };

        // Create valid coinbase reward outputs that sum to the value in the template
        let pub_key = new_pub_key();
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(625_000_000_000),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        // Test the on_new_template method
        let result =
            factory.process_new_template(new_template.clone(), coinbase_reward_outputs.clone());

        assert!(result.is_ok(), "on_new_template should succeed");

        // Get the channel and verify it has processed the template
        let channel = factory
            .get_channel(channel_id)
            .expect("Failed to get channel");

        // Verify the channel has processed the template and contains a future job
        let future_jobs = channel.get_future_jobs();
        let job_id = future_jobs.keys().next().unwrap();
        let job = future_jobs.get(job_id).unwrap();
        assert_eq!(job.get_template(), Some(&new_template));
        assert_eq!(job.get_coinbase_reward_outputs(), &coinbase_reward_outputs);

        // assert there is still no active job
        assert!(
            channel.get_active_job().is_none(),
            "Channel should not have an active job"
        );

        let set_new_prev_hash_wrong_template_id = SetNewPrevHash {
            template_id: 10000,
            prev_hash: u256_from_int(0xFFFFFFFFFFFFFFFF_u64),
            n_bits: 0,
            header_timestamp: 0,
            target: u256_from_int(0xFFFFFFFFFFFFFFFF_u64),
        };

        let result = factory.process_set_new_prev_hash(set_new_prev_hash_wrong_template_id);

        assert!(result.is_err());

        let set_new_prev_hash_correct_template_id = SetNewPrevHash {
            template_id: 1,
            prev_hash: u256_from_int(0x00000000_u32),
            n_bits: 0x1d00ffff,
            header_timestamp: 0x60000000,
            target: u256_from_int(0xFFFFFFFFFFFFFFFF_u64),
        };

        let result = factory.process_set_new_prev_hash(set_new_prev_hash_correct_template_id);

        assert!(
            result.is_ok(),
            "on_set_new_prev_hash should succeed with correct template id"
        );

        let channel = factory
            .get_channel(channel_id)
            .expect("Failed to get channel");

        assert!(
            channel.get_active_job().is_some(),
            "Channel should have an active job"
        );

        // Clean up
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_coinbase_reward_outputs_sum_above_template_value() {
        const SATS_AVAILABLE_IN_TEMPLATE: u64 = 625_000_000_000;

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create a test channel first
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;
        let initial_hashrate = 100.0;

        let (_, _, _, _) = factory
            .new_channel(
                "test_user".to_string(),
                initial_hashrate,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel");

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

        let result =
            factory.process_new_template(new_template.clone(), invalid_coinbase_reward_outputs);

        assert!(
            matches!(
                result.unwrap_err(),
                ExtendedChannelFactoryError::InvalidCoinbaseRewardOutputs
            ),
            "Should return InvalidCoinbaseRewardOutputs error"
        );

        // Test with valid coinbase reward outputs (sum is equal to the quantity available in
        // template)
        let valid_coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(SATS_AVAILABLE_IN_TEMPLATE),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        let result =
            factory.process_new_template(new_template.clone(), valid_coinbase_reward_outputs);

        assert!(
            result.is_ok(),
            "on_new_template should succeed with valid coinbase reward outputs"
        );

        // shutdown the factory
        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_process_set_custom_mining_job() {
        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..32, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        let user_identity = "test_user".to_string();
        let rollable_extranonce_size = 5;
        let target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 100.0;

        let (channel_id, _, _, _) = factory
            .new_channel(
                user_identity,
                nominal_hashrate,
                rollable_extranonce_size,
                target,
            )
            .expect("Failed to create channel");

        let pub_key = new_pub_key();
        let merkle_path_hash: binary_sv2::U256 = vec![0; 32].try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_hash].into();

        // Create the coinbase reward outputs that will be expected by the factory
        let coinbase_reward_outputs = vec![TxOut {
            value: Amount::from_sat(625_000_000_000),
            script_pubkey: ScriptBuf::new_p2pk(&pub_key),
        }];

        // Serialize the TxOut for use in SetCustomMiningJob
        let mut serialized_outputs = Vec::new();
        for output in &coinbase_reward_outputs {
            output
                .consensus_encode(&mut serialized_outputs)
                .expect("Failed to serialize TxOut");
        }

        let serialized_outputs: binary_sv2::B064K = serialized_outputs
            .try_into()
            .expect("Failed to convert serialized TxOut to B064K");

        let set_custom_mining_job = SetCustomMiningJob {
            channel_id,
            request_id: 1,
            token: vec![0].try_into().unwrap(),
            version: 1,
            prev_hash: u256_from_int(0x00000000_u32),
            nbits: 0x1d00ffff,
            min_ntime: 1000,
            coinbase_tx_version: 1,
            coinbase_prefix: vec![5, 0, 1, 2, 3, 4].try_into().unwrap(),
            coinbase_tx_input_n_sequence: 0,
            coinbase_tx_value_remaining: 625_000_000_000,
            coinbase_tx_locktime: 0,
            coinbase_tx_outputs: serialized_outputs.clone(),
            merkle_path: merkle_path.clone(),
        };

        // try processing a custom mining job before setting the chain tip
        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job.clone(),
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::ChainTipNotSet
        ));

        // Set chain tip first to avoid ChainTipNotSet error

        // set a future template for activation of chain tip
        let new_template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 1,
            coinbase_tx_version: 1,
            coinbase_prefix: vec![5, 0, 1, 2, 3, 4].try_into().unwrap(),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 625_000_000_000,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: serialized_outputs,
            coinbase_tx_locktime: 0,
            merkle_path: merkle_path.clone(),
        };

        factory
            .process_new_template(new_template.clone(), coinbase_reward_outputs.clone())
            .expect("Failed to process new template");

        let set_new_prev_hash = SetNewPrevHash {
            template_id: 1,
            prev_hash: u256_from_int(0x00000000_u32),
            n_bits: 0x1d00ffff,
            header_timestamp: 500,
            target: u256_from_int(0xFFFFFFFFFFFFFFFF_u64),
        };

        factory
            .process_set_new_prev_hash(set_new_prev_hash)
            .expect("Failed to set chain tip");

        let mut set_custom_mining_job_bad_channel_id = set_custom_mining_job.clone();
        set_custom_mining_job_bad_channel_id.channel_id = 10000;

        // try processing a custom mining job with a bad channel id
        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job_bad_channel_id,
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::CustomMiningJobBadChannelId
        ));

        // try processing a custom mining job with a bad prev hash
        let mut set_custom_mining_job_bad_prev_hash = set_custom_mining_job.clone();
        set_custom_mining_job_bad_prev_hash.prev_hash = u256_from_int(0x00000001_u32);

        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job_bad_prev_hash,
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::CustomMiningJobBadPrevHash
        ));

        // try processing a custom mining job with a bad nbits
        let mut set_custom_mining_job_bad_nbits = set_custom_mining_job.clone();
        set_custom_mining_job_bad_nbits.nbits = 0x1d00fffe;

        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job_bad_nbits,
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::CustomMiningJobBadNbits
        ));

        // try processing a custom mining job with a bad min_ntime (smaller than chain tip)
        let mut set_custom_mining_job_bad_ntime = set_custom_mining_job.clone();
        set_custom_mining_job_bad_ntime.min_ntime = 1;

        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job_bad_ntime,
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::CustomMiningJobBadNtime
        ));

        // try processing a custom mining job with bad coinbase reward outputs
        let mut set_custom_mining_job_bad_coinbase_reward_outputs = set_custom_mining_job.clone();
        set_custom_mining_job_bad_coinbase_reward_outputs.coinbase_tx_outputs =
            vec![0; 32].try_into().unwrap();

        let result = factory.process_set_custom_mining_job(
            set_custom_mining_job_bad_coinbase_reward_outputs,
            coinbase_reward_outputs.clone(),
        );
        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::CustomMiningJobBadCoinbaseRewardOutputs
        ));

        // Test processing the correct custom mining job
        let result =
            factory.process_set_custom_mining_job(set_custom_mining_job, coinbase_reward_outputs);

        assert!(
            result.is_ok(),
            "process_set_custom_mining_job should succeed"
        );

        let channel = factory.get_channel(channel_id).unwrap();
        let job = channel.get_active_job().unwrap();
        println!("job: {:?}", job);
        // Clean up
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

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        let user_identity = "test_user".to_string();
        let rollable_extranonce_size = 5;
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        let (channel_id, _, _, _) = factory
            .new_channel(
                user_identity,
                nominal_hashrate,
                rollable_extranonce_size,
                max_target,
            )
            .expect("Failed to create channel");

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

        let share_valid_block = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
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
        let extranonce_len = 32;
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
            end: extranonce_len,
        };

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        let user_identity = "test_user".to_string();
        let rollable_extranonce_size = 5;
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _, _) = factory
            .new_channel(
                user_identity,
                nominal_hashrate,
                rollable_extranonce_size,
                max_target,
            )
            .expect("Failed to create channel");

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

        // this share has hash 3e14d4c5de554fc9019d994cccd368de4acd1ea9ab830927d5ee207c8b7f1911
        // which does not meet the channel target
        let share_low_diff = SubmitSharesExtended {
            channel_id,
            sequence_number: 0,
            job_id: 1,
            nonce: 741057,
            ntime: 1745596971,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let result = factory.validate_share(share_low_diff);

        assert!(matches!(
            result.unwrap_err(),
            ExtendedChannelFactoryError::ShareDoesNotMeetTarget
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_valid_share() {
        let extranonce_len = 32;
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
            end: extranonce_len,
        };

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            2,    // share_batch_size
            channel_id_factory,
        );

        let user_identity = "test_user".to_string();
        let rollable_extranonce_size = 5;
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _, _) = factory
            .new_channel(
                user_identity,
                nominal_hashrate,
                rollable_extranonce_size,
                max_target,
            )
            .expect("Failed to create channel");

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

        // this share has hash 0000477669a32059c56e8d544d16593c45fdc71b95f714c3cde982b92a473c7f
        // which does not meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205 but does not
        // meet the network target 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 99959,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let result = factory.validate_share(valid_share);

        assert!(matches!(result, Ok(ShareValidationResult::Valid)));

        // get the share accounting
        let share_accounting = factory
            .get_share_accounting(channel_id)
            .expect("Failed to get share accounting");

        assert_eq!(share_accounting.get_shares_accepted(), 1);
        assert_eq!(share_accounting.get_last_share_sequence_number(), 1);
        assert!(!share_accounting.should_acknowledge());

        // another valid share with hash
        // 0000477669a32059c56e8d544d16593c45fdc71b95f714c3cde982b92a473c7f which is also
        // does meet channel target 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205
        // but does not meet network target
        // 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 19397,
            ntime: 1745611106,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        // since share_batch_size is 2, the result now is actually
        // ValidWithAcknowledgement
        let result = factory.validate_share(valid_share);

        assert!(matches!(
            result,
            Ok(ShareValidationResult::ValidWithAcknowledgement(_, _, _))
        ));

        // get the share accounting
        let share_accounting = factory
            .get_share_accounting(channel_id)
            .expect("Failed to get share accounting");

        assert_eq!(share_accounting.get_shares_accepted(), 2);
        assert_eq!(share_accounting.get_last_share_sequence_number(), 2);
        assert!(share_accounting.should_acknowledge());

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_share_validation_repeated_share() {
        let extranonce_len = 32;
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
            end: extranonce_len,
        };

        // Create a factory for testing
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(range_0, range_1, range_2, Some(pool_signature))
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            2,    // share_batch_size
            channel_id_factory,
        );

        let user_identity = "test_user".to_string();
        let rollable_extranonce_size = 5;
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let nominal_hashrate = 1_000.0;

        // at this point, channel target is:
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205

        let (channel_id, _, _, _) = factory
            .new_channel(
                user_identity,
                nominal_hashrate,
                rollable_extranonce_size,
                max_target,
            )
            .expect("Failed to create channel");

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

        // this share has hash 0000477669a32059c56e8d544d16593c45fdc71b95f714c3cde982b92a473c7f
        // which does not meet the channel target
        // 0001179d9861a761ffdadd11c307c4fc04eea3a418f7d687584e4434af158205 but does not
        // meet the network target 000000000000d7c0000000000000000000000000000000000000000000000000
        let valid_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 1,
            job_id: 1,
            nonce: 99959,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };

        let result = factory.validate_share(valid_share);

        assert!(matches!(result, Ok(ShareValidationResult::Valid)));

        // try to cheat by re-submitting the same share
        // with a different sequence number
        let repeated_share = SubmitSharesExtended {
            channel_id,
            sequence_number: 2,
            job_id: 1,
            nonce: 99959,
            ntime: 1745611105,
            version: 536870912,
            extranonce: vec![1, 0, 0, 0, 0].try_into().unwrap(),
        };
        let result = factory.validate_share(repeated_share);

        assert!(matches!(
            result,
            Err(ExtendedChannelFactoryError::DuplicateShare)
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }

    #[tokio::test]
    async fn test_set_extranonce_prefix() {
        let extranonce_prefix_factory =
            ExtranoncePrefixFactoryExtended::new(0..0, 0..8, 8..MAX_EXTRANONCE_LEN, None)
                .expect("Failed to create extranonce prefix factory");

        let channel_id_factory = ChannelIdFactory::new();

        let factory = ExtendedChannelFactory::new(
            extranonce_prefix_factory,
            1.0,  // expected_share_per_minute_per_channel
            true, // version_rolling_allowed
            10,   // share_batch_size
            channel_id_factory,
        );

        // Create a test channel first
        let max_target = u256_from_int(0xFFFFFFFFFFFFFFFF_u64);
        let min_extranonce_size = 4;
        let initial_hashrate = 100.0;

        let (channel_id, _, _, _) = factory
            .new_channel(
                "test_user".to_string(),
                initial_hashrate,
                min_extranonce_size,
                max_target.clone(),
            )
            .expect("Failed to create channel");

        let channel = factory
            .get_channel(channel_id)
            .expect("Failed to get channel");

        let initial_extranonce_prefix = channel.get_extranonce_prefix();
        let initial_extranonce_prefix_len = initial_extranonce_prefix.len();

        let new_extranonce_prefix = vec![1u8; initial_extranonce_prefix_len];

        let result = factory.set_extranonce_prefix(channel_id, new_extranonce_prefix.clone());

        assert!(result.is_ok());

        let channel = factory
            .get_channel(channel_id)
            .expect("Failed to get channel");

        let current_extranonce_prefix = channel.get_extranonce_prefix();
        assert_eq!(current_extranonce_prefix.clone(), new_extranonce_prefix);

        let new_extranonce_prefix_bad_length = vec![1u8; initial_extranonce_prefix_len - 1];

        let result = factory.set_extranonce_prefix(channel_id, new_extranonce_prefix_bad_length);

        assert!(matches!(
            result,
            Err(ExtendedChannelFactoryError::ExtranoncePrefixLengthMismatch)
        ));

        factory.shutdown().expect("Failed to shutdown factory");
    }
}
