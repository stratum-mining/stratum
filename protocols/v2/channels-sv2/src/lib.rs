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
//!
//! ## Modules
//!
//! - [`chain_tip`] — Chain tip handling and chain state tracking
//! - [`client`] — Mining client channel logic
//! - [`server`] — Mining server channel logic
//! - [`template`] — Template and coinbase management
//
//! ## Usage
//!
//! Add `channels_sv2` as a dependency to your mining pool or SV2 proxy implementation to
//! leverage robust channel state management and share validation logic.

pub mod chain_tip;
pub mod client;
mod merkle_root;
pub mod server;
mod target;
pub mod template;
