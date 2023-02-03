#![no_std]

//! # Job Negotiation Protocol
//!
//! This protocol runs between the Job Negotiator and Pool and can be
//! provided as a trusted 3rd party service for mining farms.
//!
//! Protocol flow:
//!

extern crate alloc;
mod set_coinbase;

pub use set_coinbase::SetCoinbase;
