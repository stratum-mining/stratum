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
use std::path::{Path, PathBuf};

use key_utils::Secp256k1PublicKey;
use serde::Deserialize;

/// Configuration for the Translator.
#[derive(Debug, Deserialize, Clone)]
pub struct TranslatorConfig {
    pub upstreams: Vec<Upstream>,
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
    /// The user identity/username to use when connecting to the pool.
    /// This will be appended with a counter for each mining channel (e.g., username.miner1,
    /// username.miner2).
    pub user_identity: String,
    /// Configuration settings for managing difficulty on the downstream connection.
    pub downstream_difficulty_config: DownstreamDifficultyConfig,
    /// Whether to aggregate all downstream connections into a single upstream channel.
    /// If true, all miners share one channel. If false, each miner gets its own channel.
    pub aggregate_channels: bool,
    /// The path to the log file for the Translator.
    log_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Upstream {
    /// The address of the upstream server.
    pub address: String,
    /// The port of the upstream server.
    pub port: u16,
    /// The Secp256k1 public key used to authenticate the upstream authority.
    pub authority_pubkey: Secp256k1PublicKey,
}

impl Upstream {
    /// Creates a new `UpstreamConfig` instance.
    pub fn new(address: String, port: u16, authority_pubkey: Secp256k1PublicKey) -> Self {
        Self {
            address,
            port,
            authority_pubkey,
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
        upstreams: Vec<Upstream>,
        downstream: DownstreamConfig,
        max_supported_version: u16,
        min_supported_version: u16,
        min_extranonce2_size: u16,
        user_identity: String,
        aggregate_channels: bool,
    ) -> Self {
        Self {
            upstreams,
            downstream_address: downstream.address,
            downstream_port: downstream.port,
            max_supported_version,
            min_supported_version,
            min_extranonce2_size,
            user_identity,
            downstream_difficulty_config: downstream.difficulty_config,
            aggregate_channels,
            log_file: None,
        }
    }

    pub fn set_log_dir(&mut self, log_dir: Option<PathBuf>) {
        if let Some(dir) = log_dir {
            self.log_file = Some(dir);
        }
    }
    pub fn log_dir(&self) -> Option<&Path> {
        self.log_file.as_deref()
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
