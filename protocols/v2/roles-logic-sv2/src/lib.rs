//! Provides all relevant types, traits and functions to implement a valid SV2 role.
//!
//! - For channel and job management, see [`channel_logic`], which utilizes [`job_creator`] and
//!   [`job_dispatcher`]
//! - For message handling, the traits in [`handlers`] should be implemented
//! - For basic traits every implementation should use, see [`common_properties`]
//! - Routers in [`routing_logic`] are used by the traits in `handlers` to decide which
//!   downstream/upstream to relay/send by using [`selectors`]
//! - For serializing/deserializing messages, see [`parsers`]
//! - see [`utils`] for helpers such as safe locking, target and merkle root calculations
//!
//!```txt
//! MiningDevice:
//!     common_properties::IsUpstream +
//!     common_properties::IsMiningUpstream +
//!     handlers::common::ParseUpstreamCommonMessages +
//!     handlers::mining::ParseUpstreamMiningMessages +
//!
//! Pool:
//!     common_properties::IsDownstream +
//!     common_properties::IsMiningDownstream +
//!     handlers::common::ParseDownstreamCommonMessages +
//!     handlers::mining::ParseDownstreamMiningMessages +
//!
//! ProxyDownstreamConnection:
//!     common_properties::IsDownstream +
//!     common_properties::IsMiningDownstream +
//!     handlers::common::ParseDownstreamCommonMessages +
//!     handlers::mining::ParseDownstreamMiningMessages +
//!
//! ProxyUpstreamConnection:
//!     common_properties::IsUpstream +
//!     common_properties::IsMiningUpstream +
//!     handlers::common::ParseUpstreamCommonMessages +
//!     handlers::mining::ParseUpstreamMiningMessages +
//! ```
pub mod channel_logic;
pub mod common_properties;
pub mod errors;
pub mod handlers;
pub mod job_creator;
pub mod job_dispatcher;
pub mod parsers;
pub mod routing_logic;
pub mod selectors;
pub mod utils;
pub use common_messages_sv2;
pub use errors::Error;
pub use job_declaration_sv2;
pub use mining_sv2;
pub use template_distribution_sv2;
