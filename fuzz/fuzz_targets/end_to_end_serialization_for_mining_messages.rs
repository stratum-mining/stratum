#![no_main]

mod common;
use binary_sv2::{Deserialize, GetSize, Serialize};
use mining_sv2::*;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    test_roundtrip!(CloseChannel, data);
    test_roundtrip!(NewMiningJob, data);
    test_roundtrip!(NewExtendedMiningJob, data);
    test_roundtrip!(OpenStandardMiningChannel, data);
    test_roundtrip!(OpenStandardMiningChannelSuccess, data);
    test_roundtrip!(OpenExtendedMiningChannel, data);
    test_roundtrip!(OpenExtendedMiningChannelSuccess, data);
    test_roundtrip!(OpenMiningChannelError, data);
    test_roundtrip!(SetCustomMiningJob, data);
    test_roundtrip!(SetCustomMiningJobSuccess, data);
    test_roundtrip!(SetCustomMiningJobError, data);
    test_roundtrip!(SetExtranoncePrefix, data);
    test_roundtrip!(SetGroupChannel, data);
    test_roundtrip!(SetNewPrevHash, data);
    test_roundtrip!(SetTarget, data);
    test_roundtrip!(SubmitSharesStandard, data);
    test_roundtrip!(SubmitSharesExtended, data);
    test_roundtrip!(SubmitSharesSuccess, data);
    test_roundtrip!(SubmitSharesError, data);
    test_roundtrip!(UpdateChannel, data);
    test_roundtrip!(UpdateChannelError, data);
});
