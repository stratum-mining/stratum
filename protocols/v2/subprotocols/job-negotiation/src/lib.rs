#![no_std]

//! # Job Negotiation Protocol
//!
//! This protocol runs between the Job Negotiator and Pool and can be
//! provided as a trusted 3rd party service for mining farms.
//!
//! Protocol flow:
//!

extern crate alloc;
mod coinbase_output_data_size;

pub use coinbase_output_data_size::CoinbaseOutputDataSize;
