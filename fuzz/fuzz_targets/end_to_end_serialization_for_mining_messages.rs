#![no_main]

mod common;

use arbitrary::Arbitrary;
use binary_sv2::{Deserialize, GetSize, Serialize};
use libfuzzer_sys::fuzz_target;
use mining_sv2::*;

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    CloseChannel(Vec<u8>),
    NewMiningJob(Vec<u8>),
    NewExtendedMiningJob(Vec<u8>),
    OpenStandardMiningChannel(Vec<u8>),
    OpenStandardMiningChannelSuccess(Vec<u8>),
    OpenExtendedMiningChannel(Vec<u8>),
    OpenExtendedMiningChannelSuccess(Vec<u8>),
    OpenMiningChannelError(Vec<u8>),
    SetCustomMiningJob(Vec<u8>),
    SetCustomMiningJobSuccess(Vec<u8>),
    SetCustomMiningJobError(Vec<u8>),
    SetExtranoncePrefix(Vec<u8>),
    SetGroupChannel(Vec<u8>),
    SetNewPrevHash(Vec<u8>),
    SetTarget(Vec<u8>),
    SubmitSharesStandard(Vec<u8>),
    SubmitSharesExtended(Vec<u8>),
    SubmitSharesSuccess(Vec<u8>),
    SubmitSharesError(Vec<u8>),
    UpdateChannel(Vec<u8>),
    UpdateChannelError(Vec<u8>),
}

fuzz_target!(|input: FuzzInput| {
    match input {
        FuzzInput::CloseChannel(data) => {
            test_roundtrip!(CloseChannel, data)
        }
        FuzzInput::NewMiningJob(data) => {
            test_roundtrip!(NewMiningJob, data)
        }
        FuzzInput::NewExtendedMiningJob(data) => {
            test_roundtrip!(NewExtendedMiningJob, data)
        }
        FuzzInput::OpenStandardMiningChannel(data) => {
            test_roundtrip!(OpenStandardMiningChannel, data)
        }
        FuzzInput::OpenStandardMiningChannelSuccess(data) => {
            test_roundtrip!(OpenStandardMiningChannelSuccess, data)
        }
        FuzzInput::OpenExtendedMiningChannel(data) => {
            test_roundtrip!(OpenExtendedMiningChannel, data)
        }
        FuzzInput::OpenExtendedMiningChannelSuccess(data) => {
            test_roundtrip!(OpenExtendedMiningChannelSuccess, data)
        }
        FuzzInput::OpenMiningChannelError(data) => {
            test_roundtrip!(OpenMiningChannelError, data)
        }
        FuzzInput::SetCustomMiningJob(data) => {
            test_roundtrip!(SetCustomMiningJob, data)
        }
        FuzzInput::SetCustomMiningJobSuccess(data) => {
            test_roundtrip!(SetCustomMiningJobSuccess, data)
        }
        FuzzInput::SetCustomMiningJobError(data) => {
            test_roundtrip!(SetCustomMiningJobError, data)
        }
        FuzzInput::SetExtranoncePrefix(data) => {
            test_roundtrip!(SetExtranoncePrefix, data)
        }
        FuzzInput::SetGroupChannel(data) => {
            test_roundtrip!(SetGroupChannel, data)
        }
        FuzzInput::SetNewPrevHash(data) => {
            test_roundtrip!(SetNewPrevHash, data)
        }
        FuzzInput::SetTarget(data) => {
            test_roundtrip!(SetTarget, data)
        }
        FuzzInput::SubmitSharesStandard(data) => {
            test_roundtrip!(SubmitSharesStandard, data)
        }
        FuzzInput::SubmitSharesExtended(data) => {
            test_roundtrip!(SubmitSharesExtended, data)
        }
        FuzzInput::SubmitSharesSuccess(data) => {
            test_roundtrip!(SubmitSharesSuccess, data)
        }
        FuzzInput::SubmitSharesError(data) => {
            test_roundtrip!(SubmitSharesError, data)
        }
        FuzzInput::UpdateChannel(data) => {
            test_roundtrip!(UpdateChannel, data)
        }
        FuzzInput::UpdateChannelError(data) => {
            test_roundtrip!(UpdateChannelError, data)
        }
    }
});

