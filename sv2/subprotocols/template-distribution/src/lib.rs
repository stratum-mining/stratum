//! # Stratum V2 Template Distribution Protocol Messages Crate
//!
//!
//! `template_distribution_sv2` is a Rust crate that implements a set of messages defined in the
//! Template Distribution Protocol of Stratum V2. The Template Distribution protocol can be used
//! to receive updates of the block templates to use in mining.
//!
//! ## Build Options
//! This crate can be built with the following features:
//! - `std`: Enables support for standard library features.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Job
//! Distribution](https://stratumprotocol.org/specification/07-Template-Distribution-Protocol/).

#![no_std]

extern crate alloc;

mod coinbase_output_constraints;
mod new_template;
mod request_transaction_data;
mod set_new_prev_hash;
mod submit_solution;

pub use coinbase_output_constraints::CoinbaseOutputConstraints;
pub use new_template::NewTemplate;
pub use request_transaction_data::{
    RequestTransactionData, RequestTransactionDataError, RequestTransactionDataSuccess,
};
pub use set_new_prev_hash::SetNewPrevHash;
pub use submit_solution::SubmitSolution;

// Template Distribution Protocol message types.
pub const MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS: u8 = 0x70;
pub const MESSAGE_TYPE_NEW_TEMPLATE: u8 = 0x71;
pub const MESSAGE_TYPE_SET_NEW_PREV_HASH: u8 = 0x72;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA: u8 = 0x73;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS: u8 = 0x74;
pub const MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR: u8 = 0x75;
pub const MESSAGE_TYPE_SUBMIT_SOLUTION: u8 = 0x76;

// For the Template Distribution protocol, the channel bit is always unset.
pub const CHANNEL_BIT_COINBASE_OUTPUT_CONSTRAINTS: bool = false;
pub const CHANNEL_BIT_NEW_TEMPLATE: bool = false;
pub const CHANNEL_BIT_SET_NEW_PREV_HASH: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS: bool = false;
pub const CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR: bool = false;
pub const CHANNEL_BIT_SUBMIT_SOLUTION: bool = false;

// Commonly used RequestTransactionDataError error_code values.
pub const ERROR_CODE_REQUEST_TRANSACTION_DATA_STALE_TEMPLATE_ID: &str = "stale-template-id";
pub const ERROR_CODE_REQUEST_TRANSACTION_DATA_TEMPLATE_ID_NOT_FOUND: &str = "template-id-not-found";