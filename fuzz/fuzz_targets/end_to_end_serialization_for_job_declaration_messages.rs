#![no_main]

mod common;
mod generators;
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
            test_roundtrip!(AllocateMiningJobToken, data, generators::gen_allocate_mining_job_token);
        }
        FuzzInput::AllocateMiningJobTokenSuccess(data) => {
            test_roundtrip!(AllocateMiningJobTokenSuccess, data, generators::gen_allocate_mining_job_token_success);
        }
        FuzzInput::DeclareMiningJob(data) => {
            test_roundtrip!(DeclareMiningJob, data, generators::gen_declare_mining_job);
        }
        FuzzInput::DeclareMiningJobSuccess(data) => {
            test_roundtrip!(DeclareMiningJobSuccess, data, generators::gen_declare_mining_job_success);
        }
        FuzzInput::DeclareMiningJobError(data) => {
            test_roundtrip!(DeclareMiningJobError, data, generators::gen_declare_mining_job_error);
        }
        FuzzInput::ProvideMissingTransactions(data) => {
            test_roundtrip!(ProvideMissingTransactions, data, generators::gen_provide_missing_transactions);
        }
        FuzzInput::ProvideMissingTransactionsSuccess(data) => {
            test_roundtrip!(ProvideMissingTransactionsSuccess, data, generators::gen_provide_missing_transactions_success);
        }
        FuzzInput::PushSolution(data) => {
            test_roundtrip!(PushSolution, data, generators::gen_push_solution);
        }
    }
});
