#![no_std]

//! # Template Distribution Protocol
//! The Template Distribution protocol is used to receive updates of the block template to use in
//! mining the next block. It effectively replaces BIPs [22](TODO) and [23](TODO) (getblocktemplate) and provides
//! a much more efficient API which allows Bitcoin Core (or some other full node software) to push
//! template updates at more appropriate times as well as provide a template which may be
//! mined on quickly for the block-after-next. While not recommended, the template update
//! protocol can be a remote server, and is thus authenticated and signed in the same way as all
//! other protocols ([using the same SetupConnection handshake][TODO]).
//! Like the [Job Negotiation](TODO) and [Job Distribution](TODO) (sub)protocols, all Template Distribution messages
//! have the channel_msg bit unset, and there is no concept of channels. After the initial common
//! handshake, the client MUST immediately send a [`CoinbaseOutputDataSize`] message to indicate
//! the space it requires for coinbase output addition, to which the server MUST immediately reply
//! with the current best block template it has available to the client. Thereafter, the server
//! SHOULD push new block templates to the client whenever the total fee in the current block
//! template increases materially, and MUST send updated block templates whenever it learns of
//! a new block.
//! Template Providers MUST attempt to broadcast blocks which are mined using work they
//! provided, and thus MUST track the work which they provided to clients.
extern crate alloc;

#[cfg(feature = "prop_test")]
use alloc::vec;
#[cfg(feature = "prop_test")]
use core::convert::TryInto;
#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

mod coinbase_output_data_size;
mod new_template;
mod request_transaction_data;
mod set_new_prev_hash;
mod submit_solution;
//
pub use coinbase_output_data_size::CoinbaseOutputDataSize;
#[cfg(not(feature = "with_serde"))]
pub use new_template::CNewTemplate;
pub use new_template::NewTemplate;
#[cfg(not(feature = "with_serde"))]
pub use request_transaction_data::{CRequestTransactionDataError, CRequestTransactionDataSuccess};
pub use request_transaction_data::{
    RequestTransactionData, RequestTransactionDataError, RequestTransactionDataSuccess,
};
#[cfg(not(feature = "with_serde"))]
pub use set_new_prev_hash::CSetNewPrevHash;
pub use set_new_prev_hash::SetNewPrevHash;
#[cfg(not(feature = "with_serde"))]
pub use submit_solution::CSubmitSolution;
pub use submit_solution::SubmitSolution;

#[no_mangle]
pub extern "C" fn _c_export_coinbase_out(_a: CoinbaseOutputDataSize) {}

#[no_mangle]
pub extern "C" fn _c_export_req_tx_data(_a: RequestTransactionData) {}

#[cfg(feature = "prop_test")]
impl NewTemplate<'static> {
    pub fn from_random(g: &mut Gen) -> Self {
        let coinbase_prefix: binary_sv2::B0255 = vec::Vec::<u8>::arbitrary(g).try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::B064K =
            vec::Vec::<u8>::arbitrary(g).try_into().unwrap();

        let merkle_path_inner = binary_sv2::U256::from_random(g);
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
impl CoinbaseOutputDataSize {
    pub fn from_random(g: &mut Gen) -> Self {
        coinbase_output_data_size::CoinbaseOutputDataSize {
            coinbase_output_max_additional_size: u32::arbitrary(g).try_into().unwrap(),
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionData {
    pub fn from_random(g: &mut Gen) -> Self {
        RequestTransactionData {
            template_id: u64::arbitrary(g).try_into().unwrap(),
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionDataError<'static> {
    pub fn from_random(g: &mut Gen) -> Self {
        let mut error_code_generator = Gen::new(255);
        let error_code: binary_sv2::Str0255 = vec::Vec::<u8>::arbitrary(&mut error_code_generator)
            .try_into()
            .unwrap();

        RequestTransactionDataError {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            error_code,
        }
    }
}

#[cfg(feature = "prop_test")]
impl RequestTransactionDataSuccess<'static> {
    pub fn from_random(g: &mut Gen) -> Self {
        let excess_data: binary_sv2::B064K = vec::Vec::<u8>::arbitrary(g).try_into().unwrap();
        let transaction_list_inner = binary_sv2::B016M::from_random(g);
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
    pub fn from_random(g: &mut Gen) -> Self {
        let prev_hash = binary_sv2::U256::from_random(g);
        let target = binary_sv2::U256::from_random(g);
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
#[derive(Clone, Debug)]
pub struct CompletelyRandomSubmitSolution(pub SubmitSolution<'static>);

#[cfg(feature = "prop_test")]
impl Arbitrary for CompletelyRandomSubmitSolution {
    fn arbitrary(g: &mut Gen) -> Self {
        let coinbase_tx: binary_sv2::B064K = vec::Vec::<u8>::arbitrary(g).try_into().unwrap();
        CompletelyRandomSubmitSolution(SubmitSolution {
            template_id: u64::arbitrary(g).try_into().unwrap(),
            version: u32::arbitrary(g).try_into().unwrap(),
            header_timestamp: u32::arbitrary(g).try_into().unwrap(),
            header_nonce: u32::arbitrary(g).try_into().unwrap(),
            coinbase_tx,
        })
    }
}
