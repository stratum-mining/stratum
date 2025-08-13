use crate::error::{Result, StratumTranslationError};
use mining_sv2::{OpenExtendedMiningChannel, SubmitSharesExtended, Target};
use v1::{client_to_server, utils::HexU32Be};

/// Builds an SV2 `OpenExtendedMiningChannel` message from the provided inputs.
///
/// # Arguments
/// * `request_id` - Unique identifier for the channel open request.
/// * `user_identity` - String representing the client's user identity.
/// * `nominal_hash_rate` - The client's nominal hashrate in H/s (as a float).
/// * `max_target` - The maximum target (difficulty) for the channel.
/// * `min_extranonce_size` - The minimum extranonce2 size required by the client.
///
/// # Returns
/// * `Ok(OpenExtendedMiningChannel)` if the message is constructed successfully.
/// * `Err(())` if any input is invalid or conversion fails.
pub fn build_sv2_open_extended_mining_channel(
    request_id: u32,
    user_identity: String,
    nominal_hash_rate: f32,
    max_target: Target,
    min_extranonce_size: u16,
) -> Result<OpenExtendedMiningChannel<'static>> {
    Ok(OpenExtendedMiningChannel {
        request_id,
        user_identity: user_identity
            .clone()
            .try_into()
            .map_err(|_| StratumTranslationError::InvalidUserIdentity(user_identity))?,
        nominal_hash_rate,
        max_target: max_target.into(),
        min_extranonce_size,
    })
}

/// Builds an SV2 `SubmitSharesExtended` from an SV1 `mining.submit`.
///
/// # Arguments
/// * `submit` - Reference to the SV1 `mining.submit` message to convert.
/// * `channel_id` - The SV2 channel ID associated with this share submission.
/// * `sequence_number` - The SV2 sequence number for this share submission.
/// * `job_version` - The SV2 job version (from the last job sent to the client).
/// * `version_rolling_mask` - Optional SV1 version rolling mask, used to compute the SV2 version
///   field.
///
/// # Returns
/// * `Ok(SubmitSharesExtended)` if the conversion is successful.
/// * `Err(())` if any required field is missing or conversion fails.
pub fn build_sv2_submit_shares_extended_from_sv1_submit(
    submit: &client_to_server::Submit<'_>,
    channel_id: u32,
    sequence_number: u32,
    job_version: u32,
    version_rolling_mask: Option<HexU32Be>,
) -> Result<SubmitSharesExtended<'static>> {
    let version = match (submit.version_bits.clone(), version_rolling_mask) {
        (Some(version_bits), Some(rolling_mask)) => {
            (job_version & !rolling_mask.0) | (version_bits.0 & rolling_mask.0)
        }
        (None, None) => job_version,
        _ => return Err(StratumTranslationError::IncompatibleVersionRollingMask),
    };

    let extranonce: Vec<u8> = submit.extra_nonce2.clone().into();
    let submit_share_extended = SubmitSharesExtended {
        channel_id,
        sequence_number,
        job_id: submit
            .job_id
            .parse::<u32>()
            .map_err(|_| StratumTranslationError::InvalidJobId)?,
        nonce: submit.nonce.0,
        ntime: submit.time.0,
        version,
        extranonce: extranonce
            .try_into()
            .map_err(|_| StratumTranslationError::InvalidExtranonceLength)?,
    };
    Ok(submit_share_extended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use v1::{client_to_server::Submit, utils::HexU32Be};

    fn submit_template() -> Submit<'static> {
        Submit {
            user_name: "w".to_string(),
            job_id: "1".to_string(),
            extra_nonce2: v1::utils::Extranonce::try_from(vec![0, 1, 2, 3]).unwrap(),
            time: HexU32Be(0),
            nonce: HexU32Be(0),
            version_bits: Some(HexU32Be(0)),
            id: 0,
        }
    }

    #[test]
    fn test_build_sv2_submit_from_sv1_submit_happy() {
        let s = submit_template();
        let res = build_sv2_submit_shares_extended_from_sv1_submit(
            &s,
            1,
            100,
            0x20000000,
            Some(HexU32Be(0x1fffe000)),
        );
        assert!(res.is_ok());

        let submit = res.unwrap();
        assert_eq!(submit.channel_id, 1);
        assert_eq!(submit.sequence_number, 100);
        assert_eq!(submit.job_id, 1); // from job_id "1" in template
        assert_eq!(submit.nonce, 0);
        assert_eq!(submit.ntime, 0);
        // Version should be computed from job_version and version rolling
        assert_eq!(submit.version, 0x20000000); // (0x20000000 & !0x1fffe000) | (0 & 0x1fffe000)
        assert_eq!(submit.extranonce.len(), 4); // from vec![0, 1, 2, 3]
    }

    #[test]
    fn test_build_sv2_submit_from_sv1_submit_incompatible_mask() {
        let mut s = submit_template();
        s.version_bits = None;
        let res = build_sv2_submit_shares_extended_from_sv1_submit(&s, 1, 1, 0, Some(HexU32Be(0)));
        assert!(res.is_err());

        // Verify it's the specific error we expect
        if let Err(e) = res {
            assert!(matches!(
                e,
                StratumTranslationError::IncompatibleVersionRollingMask
            ));
        }
    }

    #[test]
    fn test_build_sv2_submit_no_version_bits_no_mask() {
        let mut s = submit_template();
        s.version_bits = None;
        let res = build_sv2_submit_shares_extended_from_sv1_submit(&s, 5, 10, 0x20000000, None);
        assert!(res.is_ok());

        let submit = res.unwrap();
        assert_eq!(submit.channel_id, 5);
        assert_eq!(submit.sequence_number, 10);
        assert_eq!(submit.version, 0x20000000); // Should use job_version directly
    }

    #[test]
    fn test_build_sv2_submit_invalid_job_id() {
        let mut s = submit_template();
        s.job_id = "invalid_number".to_string();
        s.version_bits = None; // Ensure version compatibility
        let res = build_sv2_submit_shares_extended_from_sv1_submit(&s, 1, 1, 0, None);
        assert!(res.is_err());

        if let Err(e) = res {
            assert!(matches!(e, StratumTranslationError::InvalidJobId));
        }
    }

    #[test]
    fn test_build_sv2_open_extended_mining_channel_happy() {
        let max_target: Target = [0xffu8; 32].into();
        let res = build_sv2_open_extended_mining_channel(
            123,
            "user.worker1".to_string(),
            1000.5,
            max_target,
            8,
        );
        assert!(res.is_ok());

        let channel = res.unwrap();
        assert_eq!(channel.request_id, 123);
        assert_eq!(channel.nominal_hash_rate, 1000.5);
        assert_eq!(channel.min_extranonce_size, 8);
        // user_identity and max_target should be properly set but are internal types
    }

    #[test]
    fn test_build_sv2_open_extended_mining_channel_invalid_user() {
        let max_target: Target = [0xffu8; 32].into();
        // Create a user identity that's too long (> 255 chars)
        let long_user = "x".repeat(300);
        let res = build_sv2_open_extended_mining_channel(1, long_user, 1.0, max_target, 8);
        assert!(res.is_err());

        if let Err(e) = res {
            assert!(matches!(e, StratumTranslationError::InvalidUserIdentity(_)));
        }
    }
}
