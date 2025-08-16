//! Stratum Translation
//!
//! A small, runtime-free library that provides pure SV1↔SV2 translation helpers
//! that can be reused by different apps (translator proxy, firmware) without
//! pulling in async runtimes or networking.
//!
//! What it contains:
//! - Converters from SV2 messages/values to SV1 messages (e.g. SetTarget → mining.set_difficulty)
//! - Converters from SV1 messages/values to SV2 messages (e.g. mining.submit →
//!   SubmitSharesExtended)
//! - Uses existing utilities from channels_sv2 (e.g. target_to_difficulty)
//!
//! What it does not contain:
//! - Networking, async runtimes, channels, or long-running tasks
//!
//! Error handling:
//! - All public functions return `Result<_, error::StratumTranslationError>` with specific error
//!   kinds to aid debugging and integration.

pub mod error;
pub mod sv1_to_sv2;
pub mod sv2_to_sv1;
