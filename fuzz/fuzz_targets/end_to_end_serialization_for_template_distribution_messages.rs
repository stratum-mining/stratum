#![no_main]

mod common;
use binary_sv2::{Deserialize, GetSize, Serialize};
use template_distribution_sv2::*;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    test_roundtrip!(CoinbaseOutputConstraints, data);
    test_roundtrip!(NewTemplate, data);
    test_roundtrip!(RequestTransactionData, data);
    test_roundtrip!(RequestTransactionDataSuccess, data);
    test_roundtrip!(RequestTransactionDataError, data);
    test_roundtrip!(SetNewPrevHash, data);
    test_roundtrip!(SubmitSolution, data);
});
