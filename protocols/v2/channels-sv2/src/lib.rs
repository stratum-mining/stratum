//! # Stratum V2 Channels
//!
//! `channels_sv2` provides primitives and abstractions for Stratum V2 (Sv2) Channels.
//!
//! This crate implements the core channel management functionality for both mining clients and
//! servers, including standard, extended, and group channels, and share accounting mechanisms.
//!
//! ## Features
//!
//! - Channel primitives for SV2 mining protocol
//! - Channel management for mining servers and clients
//! - Standard, extended, and group channel support
//! - Share accounting
//! - Job store abstractions

pub mod bip141;
pub mod chain_tip;
pub mod client;
mod merkle_root;
pub mod server;
pub mod target;
pub mod template;
