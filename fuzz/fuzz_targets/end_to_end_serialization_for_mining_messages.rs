#![no_main]

mod common;
mod generators;

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
            test_roundtrip!(CloseChannel, data, generators::gen_close_channel);
        }
        FuzzInput::NewMiningJob(data) => {
            test_roundtrip!(NewMiningJob, data, generators::gen_new_mining_job);
        }
        FuzzInput::NewExtendedMiningJob(data) => {
            test_roundtrip!(NewExtendedMiningJob, data, generators::gen_new_extended_mining_job);
        }
        FuzzInput::OpenStandardMiningChannel(data) => {
            test_roundtrip!(OpenStandardMiningChannel, data, generators::gen_open_standard_mining_channel);
        }
        FuzzInput::OpenStandardMiningChannelSuccess(data) => {
            test_roundtrip!(OpenStandardMiningChannelSuccess, data, generators::gen_open_standard_mining_channel_success);
        }
        FuzzInput::OpenExtendedMiningChannel(data) => {
            test_roundtrip!(OpenExtendedMiningChannel, data, generators::gen_open_extended_mining_channel);
        }
        FuzzInput::OpenExtendedMiningChannelSuccess(data) => {
            test_roundtrip!(OpenExtendedMiningChannelSuccess, data, generators::gen_open_extended_mining_channel_success);
        }
        FuzzInput::OpenMiningChannelError(data) => {
            test_roundtrip!(OpenMiningChannelError, data, generators::gen_open_mining_channel_error);
        }
        FuzzInput::SetCustomMiningJob(data) => {
            test_roundtrip!(SetCustomMiningJob, data, generators::gen_set_custom_mining_job);
        }
        FuzzInput::SetCustomMiningJobSuccess(data) => {
            test_roundtrip!(SetCustomMiningJobSuccess, data, generators::gen_set_custom_mining_job_success);
        }
        FuzzInput::SetCustomMiningJobError(data) => {
            test_roundtrip!(SetCustomMiningJobError, data, generators::gen_set_custom_mining_job_error);
        }
        FuzzInput::SetExtranoncePrefix(data) => {
            test_roundtrip!(SetExtranoncePrefix, data, generators::gen_set_extranonce_prefix);
        }
        FuzzInput::SetGroupChannel(data) => {
            test_roundtrip!(SetGroupChannel, data, generators::gen_set_group_channel);
        }
        FuzzInput::SetNewPrevHash(data) => {
            test_roundtrip!(SetNewPrevHash, data, generators::gen_set_new_prev_hash_mining);
        }
        FuzzInput::SetTarget(data) => {
            test_roundtrip!(SetTarget, data, generators::gen_set_target);
        }
        FuzzInput::SubmitSharesStandard(data) => {
            test_roundtrip!(SubmitSharesStandard, data, generators::gen_submit_shares_standard);
        }
        FuzzInput::SubmitSharesExtended(data) => {
            test_roundtrip!(SubmitSharesExtended, data, generators::gen_submit_shares_extended);
        }
        FuzzInput::SubmitSharesSuccess(data) => {
            test_roundtrip!(SubmitSharesSuccess, data, generators::gen_submit_shares_success);
        }
        FuzzInput::SubmitSharesError(data) => {
            test_roundtrip!(SubmitSharesError, data, generators::gen_submit_shares_error);
        }
        FuzzInput::UpdateChannel(data) => {
            test_roundtrip!(UpdateChannel, data, generators::gen_update_channel);
        }
        FuzzInput::UpdateChannelError(data) => {
            test_roundtrip!(UpdateChannelError, data, generators::gen_update_channel_error);
        }
    }
});
