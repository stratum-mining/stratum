#![no_main]

mod common;

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
            test_roundtrip!(CoinbaseOutputConstraints, data)
        }
        FuzzInput::NewTemplate(data) => {
            test_roundtrip!(NewTemplate, data)
        }
        FuzzInput::RequestTransactionData(data) => {
            test_roundtrip!(RequestTransactionData, data)
        }
        FuzzInput::RequestTransactionDataSuccess(data) => {
            test_roundtrip!(RequestTransactionDataSuccess, data)
        }
        FuzzInput::RequestTransactionDataError(data) => {
            test_roundtrip!(RequestTransactionDataError, data)
        }
        FuzzInput::SetNewPrevHash(data) => {
            test_roundtrip!(SetNewPrevHash, data)
        }
        FuzzInput::SubmitSolution(data) => {
            test_roundtrip!(SubmitSolution, data)
        }
    }
});

