use alloc::{string::ToString, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, Str0255, U32AsRef, B032, U256};
use core::{convert::TryInto, fmt};
/// Message used by a downstream to request opening a Standard Channel.
///
/// Upon receiving `SetupConnectionSuccess` message, the downstream should open channel(s) on the
/// connection within a reasonable period, otherwise the upstream should close the connection for
/// inactivity.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenStandardMiningChannel<'decoder> {
    /// Specified by downstream role.
    ///
    /// Used for matching responses from upstream.
    ///
    /// The value must be connection-wide unique and is not interpreted by the upstream.
    pub request_id: U32AsRef<'decoder>,
    /// Unconstrained sequence of bytes.
    ///
    /// Whatever is needed by upstream role to identify/authenticate the downstream, e.g.
    /// “test.worker1”.
    ///
    /// Additional restrictions can be imposed by the upstream role (e.g. a pool). It is highly
    /// recommended to use UTF-8 encoding.
    pub user_identity: Str0255<'decoder>,
    /// Expected hash rate of the device (or cumulative hashrate on the channel if multiple devices
    /// are connected downstream) in h/s.
    ///
    /// Depending on upstream’s target setting policy, this value can be used for setting a
    /// reasonable target for the channel.
    ///
    /// Proxy must send 0.0f when there are no mining devices connected yet.
    pub nominal_hash_rate: f32,
    /// Maximum target which can be accepted by the connected device(s).
    ///
    /// Upstream must accept the target or respond by sending [`OpenMiningChannelError`] message.
    pub max_target: U256<'decoder>,
}

impl fmt::Display for OpenStandardMiningChannel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OpenStandardMiningChannel(request_id: {}, user_identity: {}, nominal_hash_rate: {}, max_target: {})",
            self.request_id,
            self.user_identity.as_utf8_or_hex(),
            self.nominal_hash_rate,
            self.max_target
        )
    }
}

impl OpenStandardMiningChannel<'_> {
    pub fn get_request_id_as_u32(&self) -> u32 {
        (&self.request_id).into()
    }

    pub fn update_id(&mut self, new_id: u32) {
        let bytes_new = new_id.to_le_bytes();
        let bytes_old = self.request_id.inner_as_mut();
        bytes_old[0] = bytes_new[0];
        bytes_old[1] = bytes_new[1];
        bytes_old[2] = bytes_new[2];
        bytes_old[3] = bytes_new[3];
    }
}

/// Message used by upstream to accept [`OpenStandardMiningChannel`] request from downstream.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OpenStandardMiningChannelSuccess<'decoder> {
    /// Used for matching requests/responses.
    ///
    /// Specified by downstream role and should be extracted from the corresponding
    /// [`OpenStandardMiningChannel`] message.
    pub request_id: U32AsRef<'decoder>,
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection.
    ///
    /// This will also be used for broadcasting new jobs by [`crate::NewMiningJob`].
    pub channel_id: u32,
    /// Initial target for the mining channel.
    pub target: U256<'decoder>,
    /// Bytes used as implicit first part of extranonce for the scenario when the job is served by
    /// the downstream role for a set of standard channels that belong to the same group.
    pub extranonce_prefix: B032<'decoder>,
    /// Group channel into which the new channel belongs. See SetGroupChannel for details.
    pub group_channel_id: u32,
}

impl fmt::Display for OpenStandardMiningChannelSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OpenStandardMiningChannelSuccess(request_id: {}, channel_id: {}, target: {}, extranonce_prefix: {}, group_channel_id: {})",
            self.request_id,
            self.channel_id,
            self.target,
            self.extranonce_prefix,
            self.group_channel_id
        )
    }
}

impl OpenStandardMiningChannelSuccess<'_> {
    pub fn get_request_id_as_u32(&self) -> u32 {
        (&self.request_id).into()
    }

    pub fn update_id(&mut self, new_id: u32) {
        let bytes_new = new_id.to_le_bytes();
        let bytes_old = self.request_id.inner_as_mut();
        bytes_old[0] = bytes_new[0];
        bytes_old[1] = bytes_new[1];
        bytes_old[2] = bytes_new[2];
        bytes_old[3] = bytes_new[3];
    }
}

/// Message used by a downstream to request opening an Extended Channel with an upstream role.
///
/// Similar to [`OpenStandardMiningChannel`] but requests to open an Extended Channel instead of
/// standard channel.
///
/// The main difference is the extranonce size is not fixed for a Extended Channel and can be set
/// by the upstream role based on the [`OpenExtendedMiningChannel::min_extranonce_size`] requested
/// by the downstream.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OpenExtendedMiningChannel<'decoder> {
    /// Specified by downstream role.
    ///
    /// Used for matching responses from upstream.
    ///
    /// The value must be connection-wide unique and is not interpreted by the upstream.
    pub request_id: u32,
    /// Unconstrained sequence of bytes.
    ///
    /// Whatever is needed by upstream role to identify/authenticate the downstream, e.g.
    /// “name.worker1”.
    ///
    /// Additional restrictions can be imposed by the upstream role (e.g. a pool). It is highly
    /// recommended to use UTF-8 encoding.
    pub user_identity: Str0255<'decoder>,
    /// Expected hash rate of the device (or cumulative hashrate on the channel if multiple devices
    /// are connected downstream) in h/s.
    ///
    /// Depending on upstream’s target setting policy, this value can be used for setting a
    /// reasonable target for the channel.
    ///
    /// Proxy must send 0.0f when there are no mining devices connected yet.
    pub nominal_hash_rate: f32,
    /// Maximum target which can be accepted by the connected device or devices.
    ///
    /// Upstream must accept the target or respond by sending [`OpenMiningChannelError`] message.
    pub max_target: U256<'decoder>,
    /// Minimum size of extranonce needed by the downstream device/role.
    pub min_extranonce_size: u16,
}

impl fmt::Display for OpenExtendedMiningChannel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OpenExtendedMiningChannel(request_id: {}, user_identity: {}, nominal_hash_rate: {}, max_target: {}, min_extranonce_size: {})",
            self.request_id,
            self.user_identity.as_utf8_or_hex(),
            self.nominal_hash_rate,
            self.max_target,
            self.min_extranonce_size
        )
    }
}

impl OpenExtendedMiningChannel<'_> {
    pub fn get_request_id_as_u32(&self) -> u32 {
        self.request_id
    }
}

/// Message used by upstream to accept [`OpenExtendedMiningChannel` request from downstream.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OpenExtendedMiningChannelSuccess<'decoder> {
    /// Used for matching requests/responses.
    ///
    /// Specified by downstream role and should be extracted from the corresponding
    /// [`OpenExtendedMiningChannel`] message.
    pub request_id: u32,
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection.
    ///
    /// This will also be used for broadcasting new jobs by [`crate::NewExtendedMiningJob`].
    pub channel_id: u32,
    /// Initial target for the mining channel.
    pub target: U256<'decoder>,
    /// Extranonce size (in bytes) set for the channel.
    pub extranonce_size: u16,
    /// Bytes used as implicit first part of extranonce
    pub extranonce_prefix: B032<'decoder>,
}

impl fmt::Display for OpenExtendedMiningChannelSuccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OpenExtendedMiningChannelSuccess(request_id: {}, channel_id: {}, target: {}, extranonce_size: {}, extranonce_prefix: {})",
            self.request_id,
            self.channel_id,
            self.target,
            self.extranonce_size,
            self.extranonce_prefix
        )
    }
}

/// Message used by upstream to reject [`OpenExtendedMiningChannel`] or
/// [`OpenStandardMiningchannel`] request from downstream.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenMiningChannelError<'decoder> {
    /// Used for matching requests/responses.
    ///
    /// Specified by downstream role and should be extracted from the corresponding
    /// [`OpenExtendedMiningChannel`] or [`OpenStandardMiningchannel`] message.
    pub request_id: u32,
    /// Human-readable error code(s).
    ///
    /// Possible error codes:
    ///
    /// - ‘unknown-user’
    /// - ‘max-target-out-of-range’
    pub error_code: Str0255<'decoder>,
}

impl fmt::Display for OpenMiningChannelError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OpenMiningChannelError(request_id: {}, error_code: {})",
            self.request_id,
            self.error_code.as_utf8_or_hex()
        )
    }
}

impl OpenMiningChannelError<'_> {
    pub fn new_max_target_out_of_range(request_id: u32) -> Self {
        Self {
            request_id,
            error_code: "max-target-out-of-range".to_string().try_into().unwrap(),
        }
    }
    pub fn unsupported_extranonce_size(request_id: u32) -> Self {
        Self {
            request_id,
            error_code: "unsupported-min-extranonce-size"
                .to_string()
                .try_into()
                .unwrap(),
        }
    }
    pub fn new_unknown_user(request_id: u32) -> Self {
        Self {
            request_id,
            error_code: "unknown-user".to_string().try_into().unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::tests::from_arbitrary_vec_to_array;
    use alloc::{string::String, vec::Vec};
    use core::convert::TryFrom;

    // *** OPEN STANDARD MINING CHANNEL ***
    #[quickcheck_macros::quickcheck]
    fn test_open_standard_mining_channel_fns(
        request_id: u32,
        user_identity: String,
        nominal_hash_rate: f32,
        max_target: Vec<u8>,
        new_request_id: u32,
    ) -> bool {
        let max_target: [u8; 32] = from_arbitrary_vec_to_array(max_target);
        let mut osmc = OpenStandardMiningChannel {
            request_id: U32AsRef::from(request_id),
            user_identity: Str0255::try_from(user_identity.clone())
                .expect("could not convert string to Str0255"),
            nominal_hash_rate,
            max_target: U256::from(max_target),
        };
        let test_request_id_1 = osmc.get_request_id_as_u32();
        osmc.update_id(new_request_id);
        let test_request_id_2 = osmc.get_request_id_as_u32();
        request_id == test_request_id_1
            && new_request_id == test_request_id_2
            && helpers::compare_static_osmc(osmc)
    }

    #[quickcheck_macros::quickcheck]
    fn test_open_standard_mining_channel_success(
        request_id: u32,
        channel_id: u32,
        target: Vec<u8>,
        extranonce_prefix: Vec<u8>,
        group_channel_id: u32,
        new_request_id: u32,
    ) -> bool {
        let target = from_arbitrary_vec_to_array(target);
        let extranonce_prefix = from_arbitrary_vec_to_array(extranonce_prefix);
        let mut osmcs = OpenStandardMiningChannelSuccess {
            request_id: U32AsRef::from(request_id),
            channel_id,
            target: U256::from(target),
            extranonce_prefix: B032::try_from(extranonce_prefix.to_vec()).expect(
                "OpenStandardMiningChannelSuccess: failed to convert extranonce_prefix to B032",
            ),
            group_channel_id,
        };
        let test_request_id_1 = osmcs.get_request_id_as_u32();
        osmcs.update_id(new_request_id);
        let test_request_id_2 = osmcs.get_request_id_as_u32();
        request_id == test_request_id_1 && new_request_id == test_request_id_2
    }
    // *** OPEN EXTENDED MINING CHANNEL SUCCESS ***
    #[quickcheck_macros::quickcheck]
    fn test_extended_standard_mining_channel_fns(
        request_id: u32,
        user_identity: String,
        nominal_hash_rate: f32,
        max_target: Vec<u8>,
        min_extranonce_size: u16,
    ) -> bool {
        let max_target: [u8; 32] = from_arbitrary_vec_to_array(max_target);
        let oemc = OpenExtendedMiningChannel {
            request_id,
            user_identity: Str0255::try_from(user_identity.clone())
                .expect("could not convert string to Str0255"),
            nominal_hash_rate,
            max_target: U256::from(max_target),
            min_extranonce_size,
        };
        let test_request_id_1 = oemc.get_request_id_as_u32();
        request_id == test_request_id_1
    }

    // *** HELPERS ***
    mod helpers {
        use super::*;
        pub fn compare_static_osmc(osmc: OpenStandardMiningChannel) -> bool {
            let static_osmc = OpenStandardMiningChannel::into_static(osmc.clone());
            static_osmc.request_id == osmc.request_id
                && static_osmc.user_identity == osmc.user_identity
                && static_osmc.nominal_hash_rate.to_ne_bytes()
                    == osmc.nominal_hash_rate.to_ne_bytes()
                && static_osmc.max_target == osmc.max_target
        }
    }

    // "placeholder to allow in file unit tests for quickcheck";
    #[test]
    fn test() {}
}
