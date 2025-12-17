#![no_main]

mod common;
use binary_sv2::{Deserialize, GetSize, Serialize};
use job_declaration_sv2::*;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    test_roundtrip!(AllocateMiningJobToken, data);
    test_roundtrip!(AllocateMiningJobTokenSuccess, data);
    test_roundtrip!(DeclareMiningJob, data);
    test_roundtrip!(DeclareMiningJobSuccess, data);
    test_roundtrip!(DeclareMiningJobError, data);
    test_roundtrip!(ProvideMissingTransactions, data);
    test_roundtrip!(ProvideMissingTransactionsSuccess, data);
    test_roundtrip!(PushSolution, data);
});
