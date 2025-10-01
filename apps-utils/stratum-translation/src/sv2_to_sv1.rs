//! SV2 to SV1 translation module
//!
//! This module provides functions to convert Stratum V2 (SV2) mining protocol messages
//! to Stratum V1 (SV1) format. It handles BIP141 (SegWit) data stripping and
//! protocol compatibility between the two versions.
//!
//! The main functions convert:
//! - SV2 mining jobs to SV1 notify messages
//! - SV2 difficulty targets to SV1 set_difficulty messages

use crate::error::{Result, StratumTranslationError};
use channels_sv2::{bip141::try_strip_bip141, target::target_to_difficulty};
use mining_sv2::{NewExtendedMiningJob, SetNewPrevHash, SetTarget, Target};
use tracing::debug;
use v1::{
    json_rpc, server_to_client,
    utils::{HexU32Be, MerkleNode, PrevHash},
};
/// Builds an SV1 `mining.notify` message from SV2 messages.
///
/// This function attempts to strip BIP141 (SegWit) data from the coinbase transaction
/// if present, creating a compatible SV1 mining job. If BIP141 data is not present,
/// the original job is used unchanged.
///
/// # Arguments
/// * `new_prev_hash` - The SV2 `SetNewPrevHash` message containing the new previous hash and
///   related fields.
/// * `new_job` - The SV2 `NewExtendedMiningJob` message containing the new mining job details.
/// * `clean_jobs` - Boolean indicating whether the mining jobs should be cleaned (true if a new
///   block is found).
///
/// # Returns
/// * `Ok(server_to_client::Notify<'static>)` - The constructed SV1 mining.notify message.
/// * `Err(StratumTranslationError)` - If BIP141 stripping or serialization fails.
///
/// # Errors
/// * `FailedToTryToStripBip141` - When BIP141 data stripping fails
/// * `FailedToSerializeToB064K` - When serializing stripped data to B064K format fails
pub fn build_sv1_notify_from_sv2(
    new_prev_hash: SetNewPrevHash<'static>,
    new_job: NewExtendedMiningJob<'static>,
    clean_jobs: bool,
) -> Result<server_to_client::Notify<'static>> {
    let new_job = match try_strip_bip141(
        new_job.coinbase_tx_prefix.inner_as_ref(),
        new_job.coinbase_tx_suffix.inner_as_ref(),
    )
    .map_err(StratumTranslationError::FailedToTryToStripBip141)?
    {
        Some((coinbase_tx_prefix_stripped, coinbase_tx_suffix_stripped)) => {
            // Create a new job with stripped BIP141 data
            let mut new_job_stripped = new_job.clone();
            new_job_stripped.coinbase_tx_prefix = coinbase_tx_prefix_stripped
                .try_into()
                .map_err(|_| StratumTranslationError::FailedToSerializeToB064K)?;
            new_job_stripped.coinbase_tx_suffix = coinbase_tx_suffix_stripped
                .try_into()
                .map_err(|_| StratumTranslationError::FailedToSerializeToB064K)?;
            new_job_stripped
        }
        None => new_job,
    };

    let job_id = new_job.job_id.to_string();
    let prev_hash = PrevHash(new_prev_hash.prev_hash.clone());
    let coin_base1 = new_job.coinbase_tx_prefix.to_vec().into();
    let coin_base2 = new_job.coinbase_tx_suffix.to_vec().into();
    let merkle_path = new_job.merkle_path.clone().into_static().0;
    let merkle_branch: Vec<MerkleNode> = merkle_path.into_iter().map(MerkleNode).collect();
    let version = HexU32Be(new_job.version);
    let bits = HexU32Be(new_prev_hash.nbits);
    let time = HexU32Be(if new_job.is_future() {
        new_prev_hash.min_ntime
    } else {
        new_job.min_ntime.clone().into_inner().unwrap()
    });

    let notify_response = server_to_client::Notify {
        job_id,
        prev_hash,
        coin_base1,
        coin_base2,
        merkle_branch,
        version,
        bits,
        time,
        clean_jobs,
    };
    debug!("\nNextMiningNotify: {:?}\n", notify_response);
    Ok(notify_response)
}

/// Builds an SV1 `mining.set_difficulty` JSON-RPC message from an SV2 `SetTarget`.
///
/// # Arguments
/// * `set_target` - The SV2 `SetTarget` message containing the new maximum target.
///
/// # Returns
/// * `Ok(json_rpc::Message)` - The constructed SV1 mining.set_difficulty message.
pub fn build_sv1_set_difficulty_from_sv2_set_target(
    set_target: SetTarget<'_>,
) -> Result<json_rpc::Message> {
    build_sv1_set_difficulty_from_sv2_target(set_target.maximum_target.into())
}

/// Builds an SV1 `mining.set_difficulty` JSON-RPC message from an SV2 target.
///
/// # Arguments
/// * `target` - The SV2 `Target` value to convert to SV1 set_difficulty.
///
/// # Returns
/// * `Ok(json_rpc::Message)` - The constructed SV1 mining.set_difficulty message.
pub fn build_sv1_set_difficulty_from_sv2_target(target: Target) -> Result<json_rpc::Message> {
    let value = target_to_difficulty(target);
    let set_target = v1::methods::server_to_client::SetDifficulty { value };
    Ok(set_target.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use binary_sv2::{Seq0255, Sv2Option, U256};
    use mining_sv2::{NewExtendedMiningJob, SetNewPrevHash, SetTarget as Sv2SetTarget, Target};

    fn dummy_target() -> Target {
        [0xffu8; 32].into()
    }

    #[test]
    fn test_build_sv1_set_difficulty_from_sv2_target() {
        let msg = build_sv1_set_difficulty_from_sv2_target(dummy_target())
            .expect("Should convert target to difficulty");

        // Check that we get a JSON-RPC Notification message
        assert!(matches!(msg, v1::json_rpc::Message::Notification(_)));

        if let v1::json_rpc::Message::Notification(notif) = msg {
            assert_eq!(notif.method, "mining.set_difficulty");
            assert!(!notif.params.is_null());
            // Just verify it has parameters - detailed checking would require serde_json
        }
    }

    #[test]
    fn test_build_sv1_set_difficulty_from_sv2_set_target() {
        let set_target = Sv2SetTarget {
            channel_id: 1,
            maximum_target: dummy_target().into(),
        };
        let msg = build_sv1_set_difficulty_from_sv2_set_target(set_target)
            .expect("Should convert SetTarget to difficulty");

        // Verify the result is a proper JSON-RPC Notification message
        assert!(matches!(msg, v1::json_rpc::Message::Notification(_)));

        if let v1::json_rpc::Message::Notification(notif) = msg {
            assert_eq!(notif.method, "mining.set_difficulty");
            assert!(!notif.params.is_null());
            // Just verify it has parameters - detailed checking would require serde_json
        }
    }

    #[test]
    fn test_build_sv1_notify_from_sv2_with_future_job() {
        // Test with a future job using realistic data from existing tests
        let new_prev = SetNewPrevHash {
            channel_id: 1,
            job_id: 456,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            min_ntime: 1746839904,
            nbits: 503543726,
        };

        // A future job (min_ntime is None)
        let job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 456,
            version: 536870912,
            version_rolling_allowed: true,
            merkle_path: Seq0255::new(vec![U256::from([0x03u8; 32])]).unwrap(),
            min_ntime: Sv2Option::new(None), // Future job
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
        };

        let res = build_sv1_notify_from_sv2(new_prev.into_static(), job.into_static(), true);
        assert!(res.is_ok());

        // Verify it uses prev_hash.min_ntime since job is future
        let notify = res.unwrap();
        assert_eq!(notify.time.0, 1746839904);
    }

    #[test]
    fn test_build_sv1_notify_from_sv2_with_non_future_job() {
        // Test with a non-future job using realistic data from existing tests
        let new_prev = SetNewPrevHash {
            channel_id: 1,
            job_id: 456,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            min_ntime: 1746839904,
            nbits: 503543726,
        };

        // A non-future job with realistic coinbase and merkle data from existing tests
        let job = NewExtendedMiningJob {
            channel_id: 1,
            job_id: 456,
            version: 536870912,
            version_rolling_allowed: true,
            merkle_path: Seq0255::new(vec![U256::from([0x03u8; 32])]).unwrap(),
            min_ntime: Sv2Option::new(Some(1746839905)), // Non-future job with specific timestamp
            coinbase_tx_prefix: vec![
                2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 34, 82, 0,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_suffix: vec![
                255, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
            .try_into()
            .unwrap(),
        };

        let res = build_sv1_notify_from_sv2(new_prev.into_static(), job.into_static(), false);
        assert!(res.is_ok());

        // Verify the notify message structure for non-future job
        let notify = res.unwrap();
        assert_eq!(notify.job_id, "456");
        assert!(!notify.clean_jobs); // clean_jobs set to false
        assert_eq!(notify.merkle_branch.len(), 1); // One merkle node
        assert_eq!(notify.version.0, 536870912);
        assert_eq!(notify.bits.0, 503543726);
        assert_eq!(notify.time.0, 1746839905); // Should use job's min_ntime since not future

        // Verify coinbase prefix and suffix are properly set
        assert!(!notify.coin_base1.is_empty());
        assert!(!notify.coin_base2.is_empty());
    }
}
