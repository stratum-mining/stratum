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
//! - `prop_test`: Enables support for property testing.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Job
//! Distribution](https://stratumprotocol.org/specification/07-Template-Distribution-Protocol/).

#![no_std]

extern crate alloc;

#[cfg(feature = "prop_test")]
use alloc::vec;
#[cfg(feature = "prop_test")]
use core::convert::TryInto;
#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

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

#[cfg(feature = "prop_test")]
impl NewTemplate<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let mut coinbase_prefix_gen = Gen::new(255);
        let mut coinbase_prefix: vec::Vec<u8> = vec::Vec::new();
        coinbase_prefix.resize_with(255, || u8::arbitrary(&mut coinbase_prefix_gen));
        let coinbase_prefix: binary_sv2::B0255 = coinbase_prefix.try_into().unwrap();

        let mut coinbase_tx_outputs_gen = Gen::new(64);
        let mut coinbase_tx_outputs: vec::Vec<u8> = vec::Vec::new();
        coinbase_tx_outputs.resize_with(64, || u8::arbitrary(&mut coinbase_tx_outputs_gen));
        let coinbase_tx_outputs: binary_sv2::B064K = coinbase_tx_outputs.try_into().unwrap();

        let mut merkle_path_inner_gen = Gen::new(32);
        let mut merkle_path_inner: vec::Vec<u8> = vec::Vec::new();
        merkle_path_inner.resize_with(32, || u8::arbitrary(&mut merkle_path_inner_gen));
        let merkle_path_inner: binary_sv2::U256 = merkle_path_inner.try_into().unwrap();

        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_inner].into();
        NewTemplate {
            template_id: u64::arbitrary(g),
            future_template: bool::arbitrary(g),
            version: u32::arbitrary(g),
            coinbase_tx_version: u32::arbitrary(g),
            coinbase_prefix,
            coinbase_tx_input_sequence: u32::arbitrary(g),
            coinbase_tx_value_remaining: u64::arbitrary(g),
            coinbase_tx_outputs_count: u32::arbitrary(g),
            coinbase_tx_outputs,
            coinbase_tx_locktime: u32::arbitrary(g),
            merkle_path,
        }
    }
}
#[cfg(feature = "prop_test")]
impl CoinbaseOutputConstraints {
    pub fn from_gen(g: &mut Gen) -> Self {
        CoinbaseOutputConstraints {
            coinbase_output_max_additional_size: u32::arbitrary(g).try_into().unwrap(),
            coinbase_output_max_additional_sigops: u16::arbitrary(g).try_into().unwrap(),
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionData {
    pub fn from_gen(g: &mut Gen) -> Self {
        RequestTransactionData {
            template_id: u64::arbitrary(g).try_into().unwrap(),
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionDataError<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let mut error_code_gen = Gen::new(255);
        let mut error_code: vec::Vec<u8> = vec::Vec::new();
        error_code.resize_with(255, || u8::arbitrary(&mut error_code_gen));
        let error_code: binary_sv2::Str0255 = error_code.try_into().unwrap();

        RequestTransactionDataError {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            error_code,
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionDataSuccess<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let excess_data: binary_sv2::B064K = vec::Vec::<u8>::arbitrary(g).try_into().unwrap();
        let transaction_list_inner = binary_sv2::B016M::from_gen(g);
        let transaction_list: binary_sv2::Seq064K<binary_sv2::B016M> =
            vec![transaction_list_inner].into();

        RequestTransactionDataSuccess {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            excess_data,
            transaction_list,
        }
    }
}

#[cfg(feature = "prop_test")]
impl SetNewPrevHash<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let prev_hash = binary_sv2::U256::from_gen(g);
        let target = binary_sv2::U256::from_gen(g);
        SetNewPrevHash {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            prev_hash,
            header_timestamp: u32::arbitrary(g).try_into().unwrap(),
            n_bits: u32::arbitrary(g).try_into().unwrap(),
            target,
        }
    }
}

#[cfg(feature = "prop_test")]
impl SubmitSolution<'static> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let coinbase_tx: binary_sv2::B064K = vec::Vec::<u8>::arbitrary(g).try_into().unwrap();
        SubmitSolution {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            version: u32::arbitrary(g).try_into().unwrap(),
            header_timestamp: u32::arbitrary(g).try_into().unwrap(),
            header_nonce: u32::arbitrary(g).try_into().unwrap(),
            coinbase_tx,
        }
    }
}
