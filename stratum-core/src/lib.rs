//! # Stratum Common Crate
//!
//! `stratum_core` is the central hub for all Stratum protocol crates.
//! It re-exports all the low-level v1 and v2 crates to provide a single entry point
//! for accessing the Stratum protocol implementations.
//!
//! ## Features
//!
//! - `with_buffer_pool`: Enables buffer pooling for improved memory management and performance in
//!   the binary serialization, framing, and codec layers.

pub use binary_sv2;
pub use bitcoin;
pub use buffer_sv2;
pub use channels_sv2;
pub use codec_sv2;
pub use common_messages_sv2;
pub use framing_sv2;
pub use handlers_sv2;
pub use job_declaration_sv2;
pub use mining_sv2;
pub use noise_sv2;
pub use parsers_sv2;
#[cfg(feature = "translation")]
pub use stratum_translation;
#[cfg(feature = "sv1")]
pub use sv1_api;
pub use template_distribution_sv2;
