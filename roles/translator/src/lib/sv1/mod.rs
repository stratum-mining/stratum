//! ## Downstream SV1 Module
//!
//! This module defines the structures, messages, and utility functions
//! used for handling the downstream connection with SV1 mining clients.
//!
//! It includes definitions for messages exchanged with a Bridge component,
//! structures for submitting shares and updating targets, and constants
//! and functions for managing client interactions.
//!
//! The module is organized into the following sub-modules:
//! - [`diff_management`]: (Declared here, likely contains downstream difficulty logic)
//! - [`downstream`]: Defines the core [`Downstream`] struct and its functionalities.

pub mod downstream;
pub mod sv1_server;
pub use sv1_server::sv1_server::Sv1Server;
