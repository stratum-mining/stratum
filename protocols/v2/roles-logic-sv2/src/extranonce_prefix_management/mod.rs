//! # Extranonce Prefix Management
//!
//! A `ExtranoncePrefixFactoryStandard` is a factory that guarantees unique `extranonce_prefix`
//! allocation for standard channels. It can be shared across multiple `StandardChannelFactory`
//! instances while guaranteeing unique `extranonce_prefix` allocation.
//!
//! A `ExtranoncePrefixFactoryExtended` is a factory that guarantees unique `extranonce_prefix`
//! allocation for extended channels. It can be shared across multiple `ExtendedChannelFactory`
//! instances.

pub mod error;
pub mod extended;
mod inner;
mod message;
mod response;
pub mod standard;
