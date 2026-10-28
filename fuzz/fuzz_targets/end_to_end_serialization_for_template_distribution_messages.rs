#![no_main]

mod common;
mod generators;

use arbitrary::Arbitrary;
use binary_sv2::{Deserialize, GetSize, Serialize};
use libfuzzer_sys::fuzz_target;
use template_distribution_sv2::*;

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    CoinbaseOutputConstraints(Vec<u8>),
    NewTemplate(Vec<u8>),
    RequestTransactionData(Vec<u8>),
    RequestTransactionDataSuccess(Vec<u8>),
    RequestTransactionDataError(Vec<u8>),
    SetNewPrevHash(Vec<u8>),
    SubmitSolution(Vec<u8>),
}

fuzz_target!(|input: FuzzInput| {
    match input {
        FuzzInput::CoinbaseOutputConstraints(data) => {
            test_roundtrip!(CoinbaseOutputConstraints, data, generators::gen_coinbase_output_constraints);
        }
        FuzzInput::NewTemplate(data) => {
            test_roundtrip!(NewTemplate, data, generators::gen_new_template);
        }
        FuzzInput::RequestTransactionData(data) => {
            test_roundtrip!(RequestTransactionData, data, generators::gen_request_transaction_data);
        }
        FuzzInput::RequestTransactionDataSuccess(data) => {
            test_roundtrip!(RequestTransactionDataSuccess, data, generators::gen_request_transaction_data_success);
        }
        FuzzInput::RequestTransactionDataError(data) => {
            test_roundtrip!(RequestTransactionDataError, data, generators::gen_request_transaction_data_error);
        }
        FuzzInput::SetNewPrevHash(data) => {
            test_roundtrip!(SetNewPrevHash, data, generators::gen_set_new_prev_hash_template);
        }
        FuzzInput::SubmitSolution(data) => {
            test_roundtrip!(SubmitSolution, data, generators::gen_submit_solution);
        }
    }
});
