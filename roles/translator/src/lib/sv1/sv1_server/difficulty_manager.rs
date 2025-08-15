use crate::{
    sv1::sv1_server::data::{PendingTargetUpdate, Sv1ServerData},
    utils::ShutdownMessage,
};
use async_channel::Sender;
use std::{collections::HashMap, sync::Arc, time::Duration};
use stratum_common::roles_logic_sv2::{
    mining_sv2::{SetTarget, Target, UpdateChannel},
    parsers_sv2::Mining,
    utils::{hash_rate_to_target, Mutex},
    Vardiff,
};
use stratum_translation::sv2_to_sv1::build_sv1_set_difficulty_from_sv2_target;
use tokio::{sync::broadcast, time};
use tracing::{debug, error, info, trace, warn};
use v1::json_rpc;

/// Handles all variable difficulty adjustment logic for the SV1 server.
///
/// This module contains the core vardiff implementation that:
/// - Periodically adjusts difficulty targets based on share submission rates
/// - Manages the relationship between upstream and downstream targets
/// - Handles both aggregated and non-aggregated channel modes
/// - Coordinates with the channel manager for target updates
pub struct DifficultyManager {
    shares_per_minute: f32,
    is_aggregated: bool,
}

impl DifficultyManager {
    /// Creates a new difficulty manager instance.
    ///
    /// # Arguments
    /// * `shares_per_minute` - Target shares per minute for difficulty adjustment
    /// * `is_aggregated` - Whether channels are operating in aggregated mode
    pub fn new(shares_per_minute: f32, is_aggregated: bool) -> Self {
        Self {
            shares_per_minute,
            is_aggregated,
        }
    }

    /// Spawns the variable difficulty adjustment loop.
    ///
    /// This method implements the SV1 server's variable difficulty logic for all downstreams.
    /// Every 60 seconds, this method updates the difficulty state for each downstream.
    pub async fn spawn_vardiff_loop(
        sv1_server_data: Arc<Mutex<Sv1ServerData>>,
        channel_manager_sender: Sender<Mining<'static>>,
        sv1_server_to_downstream_sender: broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
        shares_per_minute: f32,
        is_aggregated: bool,
        mut notify_shutdown: broadcast::Receiver<ShutdownMessage>,
        shutdown_complete_tx: tokio::sync::mpsc::Sender<()>,
    ) {
        let difficulty_manager = DifficultyManager::new(shares_per_minute, is_aggregated);

        'vardiff_loop: loop {
            tokio::select! {
                message = notify_shutdown.recv() => {
                    match message {
                        Ok(ShutdownMessage::ShutdownAll) => {
                            debug!("SV1 Server: Vardiff loop received shutdown signal. Exiting.");
                            break 'vardiff_loop;
                        }
                        Ok(ShutdownMessage::DownstreamShutdown(downstream_id)) => {
                            sv1_server_data.super_safe_lock(|d| {
                                d.vardiff.remove(&downstream_id);
                            });
                        }
                        Ok(ShutdownMessage::DownstreamShutdownAll) => {
                            sv1_server_data.super_safe_lock(|d|{
                                d.vardiff = HashMap::new();
                                d.downstreams = HashMap::new();
                            });
                            info!("🔌 All downstreams removed from sv1 server as upstream changed");

                            // In aggregated mode, send UpdateChannel to reflect the new state (no downstreams)
                            Self::send_update_channel_on_downstream_state_change(
                                &sv1_server_data,
                                &channel_manager_sender,
                                is_aggregated,
                            ).await;
                        }
                        Ok(ShutdownMessage::UpstreamReconnectedResetAndShutdownDownstreams) => {
                            sv1_server_data.super_safe_lock(|d|{
                                d.vardiff = HashMap::new();
                                d.downstreams = HashMap::new();
                            });
                            info!("🔌 All downstreams removed from sv1 server as upstream reconnected");

                            // In aggregated mode, send UpdateChannel to reflect the new state (no downstreams)
                            Self::send_update_channel_on_downstream_state_change(
                                &sv1_server_data,
                                &channel_manager_sender,
                                is_aggregated,
                            ).await;
                        }
                        _ => {}
                    }
                }
                _ = time::sleep(Duration::from_secs(60)) => {
                    difficulty_manager.handle_vardiff_updates(
                        &sv1_server_data,
                        &channel_manager_sender,
                        &sv1_server_to_downstream_sender,
                    ).await;
                }
            }
        }
        drop(shutdown_complete_tx);
        debug!("SV1 Server: Vardiff loop exited.");
    }

    /// Handles variable difficulty adjustments for all connected downstreams.
    ///
    /// This method implements the core vardiff logic:
    /// 1. For each downstream, calculate if a target update is needed
    /// 2. Always send UpdateChannel to keep upstream informed
    /// 3. Compare new target with upstream target to decide when to send set_difficulty:
    ///    - If new_target >= upstream_target: send set_difficulty immediately
    ///    - If new_target < upstream_target: wait for SetTarget response before sending
    ///      set_difficulty
    /// 4. Handle aggregated vs non-aggregated modes for UpdateChannel messages
    async fn handle_vardiff_updates(
        &self,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        channel_manager_sender: &Sender<Mining<'static>>,
        sv1_server_to_downstream_sender: &broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
    ) {
        let vardiff_map = sv1_server_data.super_safe_lock(|v| v.vardiff.clone());
        let mut immediate_updates = Vec::new();
        let mut all_updates = Vec::new(); // All updates will generate UpdateChannel messages

        // Process each downstream and determine update strategy
        for (downstream_id, vardiff_state) in vardiff_map.iter() {
            debug!("Updating vardiff for downstream_id: {}", downstream_id);
            let mut vardiff = vardiff_state.write().unwrap();

            // Get current state from downstream
            let Some((channel_id, hashrate, target, upstream_target)) = sv1_server_data
                .super_safe_lock(|data| {
                    data.downstreams.get(downstream_id).and_then(|ds| {
                        ds.downstream_data.super_safe_lock(|d| {
                            Some((
                                d.channel_id,
                                d.hashrate.unwrap(), /* It's safe to unwrap because we know that
                                                      * the downstream has a hashrate (we are
                                                      * doing vardiff) */
                                d.target.clone(),
                                d.upstream_target.clone(),
                            ))
                        })
                    })
                })
            else {
                continue;
            };

            let Some(channel_id) = channel_id else {
                error!("Channel id is none for downstream_id: {}", downstream_id);
                continue;
            };

            let new_hashrate_opt = vardiff.try_vardiff(hashrate, &target, self.shares_per_minute);

            if let Ok(Some(new_hashrate)) = new_hashrate_opt {
                // Calculate new target based on new hashrate
                let new_target: Target =
                    match hash_rate_to_target(new_hashrate as f64, self.shares_per_minute as f64) {
                        Ok(target) => target.into(),
                        Err(e) => {
                            error!(
                                "Failed to calculate target for hashrate {}: {:?}",
                                new_hashrate, e
                            );
                            continue;
                        }
                    };

                // Always update the downstream's pending target and hashrate
                _ = sv1_server_data.safe_lock(|dmap| {
                    if let Some(d) = dmap.downstreams.get(downstream_id) {
                        _ = d.downstream_data.safe_lock(|d| {
                            d.set_pending_target(new_target.clone());
                            d.set_pending_hashrate(Some(new_hashrate));
                        });
                    }
                });

                // All updates will be sent as UpdateChannel messages
                all_updates.push((*downstream_id, channel_id, new_target.clone(), new_hashrate));

                // Determine if we should send set_difficulty immediately or wait
                match upstream_target {
                    Some(upstream_target) => {
                        if new_target >= upstream_target {
                            // Case 1: new_target >= upstream_target, send set_difficulty
                            // immediately
                            trace!(
                                "✅ Target comparison: new_target ({:?}) >= upstream_target ({:?}) for downstream {}, will send set_difficulty immediately",
                                new_target, upstream_target, downstream_id
                            );
                            immediate_updates.push((
                                channel_id,
                                Some(*downstream_id),
                                new_target.clone(),
                            ));
                        } else {
                            // Case 2: new_target < upstream_target, delay set_difficulty until
                            // SetTarget
                            trace!(
                                "⏳ Target comparison: new_target ({:?}) < upstream_target ({:?}) for downstream {}, will delay set_difficulty until SetTarget",
                                new_target, upstream_target, downstream_id
                            );
                            // Store as pending update for when SetTarget arrives
                            sv1_server_data.super_safe_lock(|data| {
                                data.pending_target_updates.push(PendingTargetUpdate {
                                    downstream_id: *downstream_id,
                                    new_target: new_target.clone(),
                                    new_hashrate,
                                });
                            });
                        }
                    }
                    None => {
                        // No upstream target set yet, send set_difficulty immediately as fallback
                        trace!(
                            "No upstream target set for downstream {}, will send set_difficulty immediately",
                            downstream_id
                        );
                        immediate_updates.push((
                            channel_id,
                            Some(*downstream_id),
                            new_target.clone(),
                        ));
                    }
                }
            }
        }

        // Send UpdateChannel messages for ALL updates (both immediate and delayed)
        if !all_updates.is_empty() {
            self.send_update_channel_messages(all_updates, sv1_server_data, channel_manager_sender)
                .await;
        }

        // Process immediate set_difficulty updates (for new_target >= upstream_target)
        for (channel_id, downstream_id, target) in immediate_updates {
            // Send set_difficulty message immediately
            if let Ok(set_difficulty_msg) = build_sv1_set_difficulty_from_sv2_target(target) {
                if let Err(e) = sv1_server_to_downstream_sender.send((
                    channel_id,
                    downstream_id,
                    set_difficulty_msg,
                )) {
                    error!(
                        "Failed to send immediate SetDifficulty message to downstream {}: {:?}",
                        downstream_id.unwrap_or(0),
                        e
                    );
                } else {
                    trace!(
                        "Sent immediate SetDifficulty to downstream {} (new_target >= upstream_target)",
                        downstream_id.unwrap_or(0)
                    );
                }
            }
        }
    }

    /// Sends UpdateChannel messages for all target updates.
    ///
    /// Always sends UpdateChannel to keep upstream informed about target changes.
    /// Handles both aggregated and non-aggregated modes:
    /// - Aggregated: Send single UpdateChannel with minimum target and sum of hashrates
    /// - Non-aggregated: Send individual UpdateChannel for each downstream
    async fn send_update_channel_messages(
        &self,
        all_updates: Vec<(u32, u32, Target, f32)>, /* (downstream_id, channel_id, new_target,
                                                    * new_hashrate) */
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        channel_manager_sender: &Sender<Mining<'static>>,
    ) {
        if self.is_aggregated {
            // Aggregated mode: Send single UpdateChannel with minimum target and total hashrate of
            // ALL downstreams
            if let Some((_, channel_id, _, _)) = all_updates.first() {
                // Get minimum target among ALL downstreams, not just the ones with updates
                let min_target = sv1_server_data.super_safe_lock(|data| {
                    data.downstreams
                        .values()
                        .map(|downstream| {
                            downstream.downstream_data.super_safe_lock(|d| {
                                // Use pending_target if available, otherwise current target
                                d.pending_target.as_ref().unwrap_or(&d.target).clone()
                            })
                        })
                        .min()
                        .expect("At least one downstream should exist")
                });

                // Get total hashrate of ALL downstreams, not just the ones with updates
                let total_hashrate: f32 = sv1_server_data.super_safe_lock(|data| {
                    data.downstreams
                        .values()
                        .map(|downstream| {
                            downstream.downstream_data.super_safe_lock(|d| {
                                // Use pending_hashrate if available, otherwise current hashrate
                                // It's safe to unwrap because we know that the downstream has a
                                // hashrate (we are doing vardiff)
                                d.pending_hashrate.unwrap_or(d.hashrate.unwrap())
                            })
                        })
                        .sum()
                });

                let update_channel = UpdateChannel {
                    channel_id: *channel_id,
                    nominal_hash_rate: total_hashrate,
                    maximum_target: min_target.clone().into(),
                };

                debug!(
                    "Sending UpdateChannel for aggregated mode: channel_id={}, total_hashrate={} (all {} downstreams), min_target={:?}, vardiff_updates={}",
                    channel_id, total_hashrate,
                    sv1_server_data.super_safe_lock(|data| data.downstreams.len()),
                    &min_target, all_updates.len()
                );

                if let Err(e) = channel_manager_sender
                    .send(Mining::UpdateChannel(update_channel))
                    .await
                {
                    error!("Failed to send UpdateChannel message: {:?}", e);
                }
            }
        } else {
            // Non-aggregated mode: Send individual UpdateChannel for each downstream
            for (downstream_id, channel_id, new_target, new_hashrate) in &all_updates {
                let update_channel = UpdateChannel {
                    channel_id: *channel_id,
                    nominal_hash_rate: *new_hashrate,
                    maximum_target: new_target.clone().into(),
                };

                debug!(
                    "Sending UpdateChannel for downstream {}: channel_id={}, hashrate={}, target={:?}",
                    downstream_id, channel_id, new_hashrate, new_target
                );

                if let Err(e) = channel_manager_sender
                    .send(Mining::UpdateChannel(update_channel))
                    .await
                {
                    error!(
                        "Failed to send UpdateChannel message for downstream {}: {:?}",
                        downstream_id, e
                    );
                }
            }
        }
    }

    /// Handles SetTarget messages from the ChannelManager.
    ///
    /// Aggregated mode: Single SetTarget updates all downstreams and processes all pending updates
    /// Non-aggregated mode: Each SetTarget updates one specific downstream and processes its
    /// pending update
    pub async fn handle_set_target_message(
        set_target: SetTarget<'_>,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        channel_manager_sender: &Sender<Mining<'static>>,
        sv1_server_to_downstream_sender: &broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
        is_aggregated: bool,
    ) {
        let new_upstream_target: Target = set_target.maximum_target.clone().into();
        debug!(
            "Received SetTarget for channel {}: new_upstream_target = {:?}",
            set_target.channel_id, new_upstream_target
        );

        if is_aggregated {
            Self::handle_aggregated_set_target(
                new_upstream_target,
                set_target.channel_id,
                sv1_server_data,
                channel_manager_sender,
                sv1_server_to_downstream_sender,
            )
            .await;
        } else {
            Self::handle_non_aggregated_set_target(
                set_target.channel_id,
                new_upstream_target,
                sv1_server_data,
                channel_manager_sender,
                sv1_server_to_downstream_sender,
            )
            .await;
        }
    }

    /// Handles SetTarget in aggregated mode.
    /// Updates all downstreams and processes all pending set_difficulty messages.
    async fn handle_aggregated_set_target(
        new_upstream_target: Target,
        channel_id: u32,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        _channel_manager_sender: &Sender<Mining<'static>>,
        sv1_server_to_downstream_sender: &broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
    ) {
        debug!("Aggregated mode: Updating upstream target for all downstreams");

        // Update upstream target for ALL downstreams
        let downstream_ids: Vec<u32> =
            sv1_server_data.super_safe_lock(|data| data.downstreams.keys().cloned().collect());

        for downstream_id in downstream_ids {
            _ = sv1_server_data.safe_lock(|data| {
                if let Some(downstream) = data.downstreams.get(&downstream_id) {
                    _ = downstream.downstream_data.safe_lock(|d| {
                        d.set_upstream_target(new_upstream_target.clone());
                    });
                }
            });
        }

        // Process ALL pending difficulty updates that can now be sent downstream
        let applicable_updates = Self::get_pending_difficulty_updates(
            new_upstream_target,
            None,
            channel_id,
            sv1_server_data,
        );
        Self::send_pending_set_difficulty_messages_to_downstream(
            applicable_updates,
            sv1_server_data,
            sv1_server_to_downstream_sender,
        )
        .await;
    }

    /// Handles SetTarget in non-aggregated mode.
    /// Updates the specific downstream and processes its pending set_difficulty message.
    async fn handle_non_aggregated_set_target(
        channel_id: u32,
        new_upstream_target: Target,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        _channel_manager_sender: &Sender<Mining<'static>>,
        sv1_server_to_downstream_sender: &broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
    ) {
        debug!(
            "Non-aggregated mode: Processing SetTarget for channel {}",
            channel_id
        );

        let affected_downstream = sv1_server_data.super_safe_lock(|data| {
            data.downstreams
                .iter()
                .find_map(|(downstream_id, downstream)| {
                    downstream.downstream_data.super_safe_lock(|d| {
                        if d.channel_id == Some(channel_id) {
                            Some(*downstream_id)
                        } else {
                            None
                        }
                    })
                })
        });

        if let Some(downstream_id) = affected_downstream {
            // Update upstream target for this specific downstream
            _ = sv1_server_data.safe_lock(|data| {
                if let Some(downstream) = data.downstreams.get(&downstream_id) {
                    _ = downstream.downstream_data.safe_lock(|d| {
                        d.set_upstream_target(new_upstream_target.clone());
                    });
                }
            });
            trace!("Updated upstream target for downstream {}", downstream_id);

            // Process pending difficulty updates for this specific downstream only
            let applicable_updates = Self::get_pending_difficulty_updates(
                new_upstream_target,
                Some(downstream_id),
                channel_id,
                sv1_server_data,
            );
            Self::send_pending_set_difficulty_messages_to_downstream(
                applicable_updates,
                sv1_server_data,
                sv1_server_to_downstream_sender,
            )
            .await;
        } else {
            warn!("No downstream found for channel {}", channel_id);
        }
    }

    /// Gets pending updates that can now be applied based on the new upstream target.
    /// If downstream_id is provided, only returns updates for that specific downstream.
    /// Logs a warning if the upstream target is higher than any requested target.
    fn get_pending_difficulty_updates(
        new_upstream_target: Target,
        downstream_id: Option<u32>,
        channel_id: u32,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
    ) -> Vec<PendingTargetUpdate> {
        let mut applicable_updates = Vec::new();

        sv1_server_data.super_safe_lock(|data| {
            data.pending_target_updates.retain(|pending_update| {
                // Check if we should process this update
                let should_process = match downstream_id {
                    Some(downstream_id) => pending_update.downstream_id == downstream_id,
                    None => true, // Process all in aggregated mode
                };

                if should_process {
                    if pending_update.new_target >= new_upstream_target {
                        // Target is acceptable, can apply immediately
                        applicable_updates.push(pending_update.clone());
                        false // remove from pending list
                    } else {
                        // WARNING: Upstream gave us a target higher than what we requested
                        error!(
                            "❌ Protocol issue: SetTarget response has target ({:?}) which is higher than requested target ({:?}) in UpdateChannel for channel {:?}. Ignoring this pending update for downstream {:?}.",
                            new_upstream_target, pending_update.new_target, channel_id, pending_update.downstream_id
                        );
                        false // remove from pending list (don't keep invalid requests)
                    }
                } else {
                    true // keep in pending list (not relevant for this SetTarget)
                }
            });
        });
        applicable_updates
    }

    /// Sends set_difficulty messages for all applicable pending updates.
    async fn send_pending_set_difficulty_messages_to_downstream(
        difficulty_updates: Vec<PendingTargetUpdate>,
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        sv1_server_to_downstream_sender: &broadcast::Sender<(u32, Option<u32>, json_rpc::Message)>,
    ) {
        for pending_update in &difficulty_updates {
            // Get channel_id for this downstream
            let channel_id = sv1_server_data.super_safe_lock(|data| {
                data.downstreams
                    .get(&pending_update.downstream_id)
                    .and_then(|ds| ds.downstream_data.super_safe_lock(|d| d.channel_id))
            });

            if let Some(channel_id) = channel_id {
                // Send set_difficulty message
                if let Ok(set_difficulty_msg) =
                    build_sv1_set_difficulty_from_sv2_target(pending_update.new_target.clone())
                {
                    if let Err(e) = sv1_server_to_downstream_sender.send((
                        channel_id,
                        Some(pending_update.downstream_id),
                        set_difficulty_msg,
                    )) {
                        error!(
                            "Failed to send SetDifficulty to downstream {}: {:?}",
                            pending_update.downstream_id, e
                        );
                    } else {
                        trace!(
                            "Sent SetDifficulty to downstream {}",
                            pending_update.downstream_id
                        );
                    }
                }
            }
        }
    }

    /// Sends an UpdateChannel message for aggregated mode when downstream state changes
    /// (e.g., disconnect). Calculates total hashrate and minimum target among all remaining
    /// downstreams.
    pub async fn send_update_channel_on_downstream_state_change(
        sv1_server_data: &Arc<Mutex<Sv1ServerData>>,
        channel_manager_sender: &Sender<Mining<'static>>,
        is_aggregated: bool,
    ) {
        if !is_aggregated {
            return; // Only applies to aggregated mode
        }

        let (total_hashrate, min_target, channel_id, downstream_count) = sv1_server_data
            .super_safe_lock(|data| {
                // Hardcoded channel_id 0 (the ChannelManager will set this channel_id to the
                // upstream extended channel id)
                let channel_id = 0;

                let total_hashrate: f32 = data
                    .downstreams
                    .values()
                    .map(|downstream| {
                        downstream.downstream_data.super_safe_lock(|d| {
                            // Use pending_hashrate if available, otherwise current hashrate
                            // It's safe to unwrap because we know that the downstream has a
                            // hashrate (we are doing vardiff)
                            d.pending_hashrate.unwrap_or(d.hashrate.unwrap())
                        })
                    })
                    .sum();

                let min_target = data
                    .downstreams
                    .values()
                    .map(|downstream| {
                        downstream.downstream_data.super_safe_lock(|d| {
                            // Use pending_target if available, otherwise current target
                            d.pending_target.as_ref().unwrap_or(&d.target).clone()
                        })
                    })
                    .min();

                (
                    total_hashrate,
                    min_target,
                    Some(channel_id),
                    data.downstreams.len(),
                )
            });

        if let (Some(min_target), Some(channel_id)) = (min_target, channel_id) {
            let update_channel = UpdateChannel {
                channel_id,
                nominal_hash_rate: total_hashrate,
                maximum_target: min_target.clone().into(),
            };

            if let Err(e) = channel_manager_sender
                .send(Mining::UpdateChannel(update_channel))
                .await
            {
                error!(
                    "Failed to send UpdateChannel message after downstream state change: {:?}",
                    e
                );
            }
        } else if downstream_count == 0 {
            // No downstreams remaining, send UpdateChannel with maximum possible target
            let update_channel = UpdateChannel {
                channel_id: 0,
                nominal_hash_rate: 0.0, // No hashrate when no downstreams
                maximum_target: [0xFF; 32].into(),
            };

            if let Err(e) = channel_manager_sender
                .send(Mining::UpdateChannel(update_channel))
                .await
            {
                error!(
                    "Failed to send UpdateChannel message with maximum target: {:?}",
                    e
                );
            }
        } else {
            warn!("Cannot send UpdateChannel after downstream state change: no downstreams remaining or no channel_id");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sv1::sv1_server::data::Sv1ServerData;
    use async_channel::unbounded;
    use std::sync::Arc;

    fn create_test_difficulty_manager() -> DifficultyManager {
        DifficultyManager::new(5.0, true) // 5 shares per minute, aggregated mode
    }

    fn create_test_sv1_server_data() -> Arc<Mutex<Sv1ServerData>> {
        let data = Sv1ServerData::new(true); // aggregated mode
        Arc::new(Mutex::new(data))
    }

    #[test]
    fn test_difficulty_manager_creation() {
        let manager = create_test_difficulty_manager();
        assert_eq!(manager.shares_per_minute, 5.0);
        assert!(manager.is_aggregated);

        let non_agg_manager = DifficultyManager::new(10.0, false);
        assert_eq!(non_agg_manager.shares_per_minute, 10.0);
        assert!(!non_agg_manager.is_aggregated);
    }

    #[tokio::test]
    async fn test_send_update_channel_on_downstream_state_change_aggregated() {
        let sv1_server_data = create_test_sv1_server_data();
        let (sender, receiver) = unbounded();

        // Test with no downstreams
        DifficultyManager::send_update_channel_on_downstream_state_change(
            &sv1_server_data,
            &sender,
            true, // aggregated
        )
        .await;

        // Should send UpdateChannel with maximum target when no downstreams
        let received_message = receiver
            .try_recv()
            .expect("Should receive UpdateChannel message");
        if let Mining::UpdateChannel(update_channel) = received_message {
            assert_eq!(update_channel.channel_id, 0);
            assert_eq!(update_channel.nominal_hash_rate, 0.0);
            assert_eq!(update_channel.maximum_target, [0xFF; 32].into());
        } else {
            panic!(
                "Expected UpdateChannel message, got: {:?}",
                received_message
            );
        }
    }

    #[tokio::test]
    async fn test_send_update_channel_on_downstream_state_change_non_aggregated() {
        let sv1_server_data = create_test_sv1_server_data();
        let (sender, _receiver) = unbounded();

        DifficultyManager::send_update_channel_on_downstream_state_change(
            &sv1_server_data,
            &sender,
            false, // non-aggregated
        )
        .await;

        // Non-aggregated mode should return early and not crash
    }

    #[test]
    fn test_get_pending_difficulty_updates_basic() {
        let sv1_server_data = create_test_sv1_server_data();
        let upstream_target: Target = hash_rate_to_target(150.0, 5.0).unwrap().into();

        // Test with empty pending updates
        let applicable_updates = DifficultyManager::get_pending_difficulty_updates(
            upstream_target,
            None, // All downstreams
            1,    // channel_id
            &sv1_server_data,
        );

        assert_eq!(applicable_updates.len(), 0);
    }
}
