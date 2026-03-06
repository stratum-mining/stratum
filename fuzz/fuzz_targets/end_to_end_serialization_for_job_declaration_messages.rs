#![no_main]

mod common;
use arbitrary::Arbitrary;
use binary_sv2::{Deserialize, GetSize, Serialize};
use job_declaration_sv2::*;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    AllocateMiningJobToken(Vec<u8>),
    AllocateMiningJobTokenSuccess(Vec<u8>),
    DeclareMiningJob(Vec<u8>),
    DeclareMiningJobSuccess(Vec<u8>),
    DeclareMiningJobError(Vec<u8>),
    ProvideMissingTransactions(Vec<u8>),
    ProvideMissingTransactionsSuccess(Vec<u8>),
    PushSolution(Vec<u8>),
}

fuzz_target!(|input: FuzzInput| {
    match input {
        FuzzInput::AllocateMiningJobToken(data) => {
            test_roundtrip!(AllocateMiningJobToken, data)
        }
        FuzzInput::AllocateMiningJobTokenSuccess(data) => {
            test_roundtrip!(AllocateMiningJobTokenSuccess, data)
        }
        FuzzInput::DeclareMiningJob(data) => {
            test_roundtrip!(DeclareMiningJob, data)
        }
        FuzzInput::DeclareMiningJobSuccess(data) => {
            test_roundtrip!(DeclareMiningJobSuccess, data)
        }
        FuzzInput::DeclareMiningJobError(data) => {
            test_roundtrip!(DeclareMiningJobError, data)
        }
        FuzzInput::ProvideMissingTransactions(data) => {
            test_roundtrip!(ProvideMissingTransactions, data)
        }
        FuzzInput::ProvideMissingTransactionsSuccess(data) => {
            test_roundtrip!(ProvideMissingTransactionsSuccess, data)
        }
        FuzzInput::PushSolution(data) => {
            test_roundtrip!(PushSolution, data)
        }
    }
});

