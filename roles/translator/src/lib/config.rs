//! ## Translator Configuration Module
//!
//! Defines [`TranslatorConfig`], the primary configuration structure for the Translator.
//!
//! This module provides the necessary structures to configure the Translator,
//! managing connections and settings for both upstream and downstream interfaces.
//!
//! This module handles:
//! - Upstream server address, port, and authentication key ([`UpstreamConfig`])
//! - Downstream interface address and port ([`DownstreamConfig`])
//! - Supported protocol versions
//! - Downstream difficulty adjustment parameters ([`DownstreamDifficultyConfig`])
//! - Upstream difficulty adjustment parameters ([`UpstreamDifficultyConfig`])
use std::path::{Path, PathBuf};

use key_utils::Secp256k1PublicKey;
use serde::Deserialize;

/// Configuration for the Translator.
#[derive(Debug, Deserialize, Clone)]
pub struct TranslatorConfig {
    /// The address of the upstream server.
    pub upstream_address: String,
    /// The port of the upstream server.
    pub upstream_port: u16,
    /// The Secp256k1 public key used to authenticate the upstream authority.
    pub upstream_authority_pubkey: Secp256k1PublicKey,
    /// The address for the downstream interface.
    pub downstream_address: String,
    /// The port for the downstream interface.
    pub downstream_port: u16,
    /// The maximum supported protocol version for communication.
    pub max_supported_version: u16,
    /// The minimum supported protocol version for communication.
    pub min_supported_version: u16,
    /// The minimum size required for the extranonce2 field in mining submissions.
    pub min_extranonce2_size: u16,
    /// Configuration settings for managing difficulty on the downstream connection.
    pub downstream_difficulty_config: DownstreamDifficultyConfig,
    /// Configuration settings for managing difficulty on the upstream connection.
    pub upstream_difficulty_config: UpstreamDifficultyConfig,
    /// The path to the log file for the Translator.
    log_file: Option<PathBuf>,
}

impl TranslatorConfig {
    pub fn set_log_dir(&mut self, log_dir: Option<PathBuf>) {
        if let Some(dir) = log_dir {
            self.log_file = Some(dir);
        }
    }
    pub fn log_dir(&self) -> Option<&Path> {
        self.log_file.as_deref()
    }
}

/// Configuration settings specific to the upstream connection.
pub struct UpstreamConfig {
    /// The address of the upstream server.
    address: String,
    /// The port of the upstream server.
    port: u16,
    /// The Secp256k1 public key used to authenticate the upstream authority.
    authority_pubkey: Secp256k1PublicKey,
    /// Configuration settings for managing difficulty on the upstream connection.
    difficulty_config: UpstreamDifficultyConfig,
}

impl UpstreamConfig {
    /// Creates a new `UpstreamConfig` instance.
    pub fn new(
        address: String,
        port: u16,
        authority_pubkey: Secp256k1PublicKey,
        difficulty_config: UpstreamDifficultyConfig,
    ) -> Self {
        Self {
            address,
            port,
            authority_pubkey,
            difficulty_config,
        }
    }
}

/// Configuration settings specific to the downstream connection.
pub struct DownstreamConfig {
    /// The address for the downstream interface.
    address: String,
    /// The port for the downstream interface.
    port: u16,
    /// Configuration settings for managing difficulty on the downstream connection.
    difficulty_config: DownstreamDifficultyConfig,
}

impl DownstreamConfig {
    /// Creates a new `DownstreamConfig` instance.
    pub fn new(address: String, port: u16, difficulty_config: DownstreamDifficultyConfig) -> Self {
        Self {
            address,
            port,
            difficulty_config,
        }
    }
}

impl TranslatorConfig {
    /// Creates a new `TranslatorConfig` instance by combining upstream and downstream
    /// configurations and specifying version and extranonce constraints.
    pub fn new(
        upstream: UpstreamConfig,
        downstream: DownstreamConfig,
        max_supported_version: u16,
        min_supported_version: u16,
        min_extranonce2_size: u16,
    ) -> Self {
        Self {
            upstream_address: upstream.address,
            upstream_port: upstream.port,
            upstream_authority_pubkey: upstream.authority_pubkey,
            downstream_address: downstream.address,
            downstream_port: downstream.port,
            max_supported_version,
            min_supported_version,
            min_extranonce2_size,
            downstream_difficulty_config: downstream.difficulty_config,
            upstream_difficulty_config: upstream.difficulty_config,
            log_file: None,
        }
    }
}

/// Configuration settings for managing difficulty adjustments on the downstream connection.
#[derive(Debug, Deserialize, Clone)]
pub struct DownstreamDifficultyConfig {
    /// The minimum hashrate expected from an individual miner on the downstream connection.
    pub min_individual_miner_hashrate: f32,
    /// The target number of shares per minute for difficulty adjustment.
    pub shares_per_minute: f32,
    /// The number of shares submitted since the last difficulty update.
    #[serde(default = "u32::default")]
    pub submits_since_last_update: u32,
    /// The timestamp of the last difficulty update.
    #[serde(default = "u64::default")]
    pub timestamp_of_last_update: u64,
}

impl DownstreamDifficultyConfig {
    /// Creates a new `DownstreamDifficultyConfig` instance.
    pub fn new(
        min_individual_miner_hashrate: f32,
        shares_per_minute: f32,
        submits_since_last_update: u32,
        timestamp_of_last_update: u64,
    ) -> Self {
        Self {
            min_individual_miner_hashrate,
            shares_per_minute,
            submits_since_last_update,
            timestamp_of_last_update,
        }
    }
}
impl PartialEq for DownstreamDifficultyConfig {
    fn eq(&self, other: &Self) -> bool {
        other.min_individual_miner_hashrate.round() as u32
            == self.min_individual_miner_hashrate.round() as u32
    }
}

/// Configuration settings for difficulty adjustments on the upstream connection.
#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamDifficultyConfig {
    /// The interval in seconds at which the channel difficulty should be updated.
    pub channel_diff_update_interval: u32,
    /// The nominal hashrate for the channel, used in difficulty calculations.
    pub channel_nominal_hashrate: f32,
    /// The timestamp of the last difficulty update for the channel.
    #[serde(default = "u64::default")]
    pub timestamp_of_last_update: u64,
    /// Indicates whether shares from downstream should be aggregated before submitting upstream.
    #[serde(default = "bool::default")]
    pub should_aggregate: bool,
}

impl UpstreamDifficultyConfig {
    /// Creates a new `UpstreamDifficultyConfig` instance.
    pub fn new(
        channel_diff_update_interval: u32,
        channel_nominal_hashrate: f32,
        timestamp_of_last_update: u64,
        should_aggregate: bool,
    ) -> Self {
        Self {
            channel_diff_update_interval,
            channel_nominal_hashrate,
            timestamp_of_last_update,
            should_aggregate,
        }
    }
}
