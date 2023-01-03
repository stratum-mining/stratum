//! It exports traits that when implemented make the implementor a valid Sv2 role:
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
//! ProxyDownstreamConnetion:
//!     common_properties::IsDownstream +
//!     common_properties::IsMiningDownstream +
//!     handlers::common::ParseDownstreamCommonMessages +
//!     handlers::mining::ParseDownstreamMiningMessages +
//!
//! ProxyUpstreamConnetion:
//!     common_properties::IsUpstream +
//!     common_properties::IsMiningUpstream +
//!     handlers::common::ParseUpstreamCommonMessages +
//!     handlers::mining::ParseUpstreamMiningMessages +
//! ```
//!
//! In parser there is anything needed for serialize and deserialize messages.
//! Handlers export the main traits needed in order to implement a valid Sv2 role.
//! Routers in routing_logic are used by the traits in handlers for decide to which
//! downstream/upstrem realy/send they use selectors in order to do that.
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
pub use bitcoin;
pub use common_messages_sv2;
pub use errors::Error;
pub use job_negotiation_sv2;
pub use mining_sv2;
pub use template_distribution_sv2;
