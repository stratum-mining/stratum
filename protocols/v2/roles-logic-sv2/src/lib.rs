//! # Stratum V2 Roles-Logic Library
//!
//! roles_logic_sv2 provides the core logic and utilities for implementing roles in the Stratum V2
//! (Sv2) protocol, such as miners, pools, and proxies. It abstracts message handling, channel
//! management, job creation, and routing logic, enabling efficient and secure communication across
//! upstream and downstream connections.
//!
//! ## Usage
//!
//! To include this crate in your project, run:
//! ```bash
//! $ cargo add roles_logic_sv2
//! ```
//!
//! ## Build Options
//!
//! This crate can be built with the following features:
//!
//! - `prop_test`: Enables support for property testing in [`template_distribution_sv2`] crate.
pub mod errors;
pub mod handlers;
pub mod utils;
pub mod vardiff;
pub use bitcoin;
pub use channels_sv2;
pub use codec_sv2;
pub use common_messages_sv2;
pub use errors::Error;
pub use handlers_sv2;
pub use job_declaration_sv2;
pub use mining_sv2;
pub use parsers_sv2;
pub use template_distribution_sv2;
pub use vardiff::{classic::VardiffState, Vardiff};
