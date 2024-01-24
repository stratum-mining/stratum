use alloc::string::ToString;
#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::{binary_codec_sv2, U32AsRef};
use binary_sv2::{Deserialize, Serialize, Str0255, B032, U256};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;
#[cfg(feature = "with_serde")]
use core::convert::TryInto;

/// # OpenStandardMiningChannel (Client -> Server)
/// This message requests to open a standard channel to the upstream node.
/// After receiving a SetupConnection.Success message, the client SHOULD respond by opening
/// channels on the connection. If no channels are opened within a reasonable period the server
/// SHOULD close the connection for inactivity.
/// Every client SHOULD start its communication with an upstream node by opening a channel,
/// which is necessary for almost all later communication. The upstream node either passes
/// opening the channel further or has enough local information to handle channel opening on its
/// own (this is mainly intended for v1 proxies).
/// Clients must also communicate information about their hashing power in order to receive
/// well-calibrated job assignments.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenStandardMiningChannel<'decoder> {
    /// Client-specified identifier for matching responses from upstream server.
    /// The value MUST be connection-wide unique and is not interpreted by
    /// the server.
    #[cfg(not(feature = "with_serde"))]
    pub request_id: U32AsRef<'decoder>,
    #[cfg(feature = "with_serde")]
    pub request_id: u32,
    /// Unconstrained sequence of bytes. Whatever is needed by upstream
    /// node to identify/authenticate the client, e.g. “braiinstest.worker1”.
    /// Additional restrictions can be imposed by the upstream node (e.g. a
    /// pool). It is highly recommended that UTF-8 encoding is used.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identity: Str0255<'decoder>,
    /// [h/s] Expected hash rate of the device (or cumulative hashrate on the
    /// channel if multiple devices are connected downstream) in h/s.
    /// Depending on server’s target setting policy, this value can be used for
    /// setting a reasonable target for the channel. Proxy MUST send 0.0f when
    /// there are no mining devices connected yet.
    pub nominal_hash_rate: f32,
    /// Maximum target which can be accepted by the connected device or
    /// devices. Server MUST accept the target or respond by sending
    /// OpenMiningChannel.Error message.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub max_target: U256<'decoder>,
}

impl<'decoder> OpenStandardMiningChannel<'decoder> {
    #[cfg(not(feature = "with_serde"))]
    pub fn get_request_id_as_u32(&self) -> u32 {
        (&self.request_id).into()
    }

    #[cfg(feature = "with_serde")]
    pub fn get_request_id_as_u32(&self) -> u32 {
        self.request_id
    }

    #[cfg(not(feature = "with_serde"))]
    pub fn update_id(&mut self, new_id: u32) {
        let bytes_new = new_id.to_le_bytes();
        let bytes_old = self.request_id.inner_as_mut();
        bytes_old[0] = bytes_new[0];
        bytes_old[1] = bytes_new[1];
        bytes_old[2] = bytes_new[2];
        bytes_old[3] = bytes_new[3];
    }

    #[cfg(feature = "with_serde")]
    pub fn update_id(&mut self, _new_id: u32) {
        // DO NOT USE MEM SWAP HERE AS IT DO NOT UPDATE THE UNDERLING PAYLOAD
        // INSTEAD IMPLEMENT U32ASREF FOR SERDE
        todo!()
    }
}

/// # OpenStandardMiningChannel.Success (Server -> Client)
/// Sent as a response for opening a standard channel, if successful.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OpenStandardMiningChannelSuccess<'decoder> {
    /// Client-specified request ID from OpenStandardMiningChannel message,
    /// so that the client can pair responses with open channel requests.
    #[cfg(not(feature = "with_serde"))]
    pub request_id: U32AsRef<'decoder>,
    #[cfg(feature = "with_serde")]
    pub request_id: u32,
    /// Newly assigned identifier of the channel, stable for the whole lifetime of
    /// the connection. E.g. it is used for broadcasting new jobs by
    /// NewExtendedMiningJob.
    pub channel_id: u32,
    /// Initial target for the mining channel.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub target: U256<'decoder>,
    /// Bytes used as implicit first part of extranonce for the scenario when
    /// extended job is served by the upstream node for a set of standard
    /// channels that belong to the same group.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce_prefix: B032<'decoder>,
    /// Group channel into which the new channel belongs. See
    /// SetGroupChannel for details.
    pub group_channel_id: u32,
}

impl<'decoder> OpenStandardMiningChannelSuccess<'decoder> {
    #[cfg(not(feature = "with_serde"))]
    pub fn get_request_id_as_u32(&self) -> u32 {
        (&self.request_id).into()
    }

    #[cfg(feature = "with_serde")]
    pub fn get_request_id_as_u32(&self) -> u32 {
        self.request_id
    }

    #[cfg(not(feature = "with_serde"))]
    pub fn update_id(&mut self, new_id: u32) {
        let bytes_new = new_id.to_le_bytes();
        let bytes_old = self.request_id.inner_as_mut();
        bytes_old[0] = bytes_new[0];
        bytes_old[1] = bytes_new[1];
        bytes_old[2] = bytes_new[2];
        bytes_old[3] = bytes_new[3];
    }

    #[cfg(feature = "with_serde")]
    pub fn update_id(&mut self, _new_id: u32) {
        // DO NOT USE MEM SWAP HERE AS IT DO NOT UPDATE THE UNDERLING PAYLOAD
        // INSTEAD IMPLEMENT U32ASREF FOR SERDE
        todo!()
    }
}

/// # OpenExtendedMiningChannel (Client -> Server)
/// Similar to *OpenStandardMiningChannel* but requests to open an extended channel instead of
/// standard channel.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OpenExtendedMiningChannel<'decoder> {
    /// Client-specified identifier for matching responses from upstream server.
    /// The value MUST be connection-wide unique and is not interpreted by
    /// the server.
    pub request_id: u32,
    /// Unconstrained sequence of bytes. Whatever is needed by upstream
    /// node to identify/authenticate the client, e.g. “braiinstest.worker1”.
    /// Additional restrictions can be imposed by the upstream node (e.g. a
    /// pool). It is highly recommended that UTF-8 encoding is used.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub user_identity: Str0255<'decoder>,
    /// [h/s] Expected hash rate of the device (or cumulative hashrate on the
    /// channel if multiple devices are connected downstream) in h/s.
    /// Depending on server’s target setting policy, this value can be used for
    /// setting a reasonable target for the channel. Proxy MUST send 0.0f when
    /// there are no mining devices connected yet.
    pub nominal_hash_rate: f32,
    /// Maximum target which can be accepted by the connected device or
    /// devices. Server MUST accept the target or respond by sending
    /// OpenMiningChannel.Error message.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub max_target: U256<'decoder>,
    /// Minimum size of extranonce needed by the device/node.
    pub min_extranonce_size: u16,
}
impl<'decoder> OpenExtendedMiningChannel<'decoder> {
    pub fn get_request_id_as_u32(&self) -> u32 {
        self.request_id
    }
}

/// # OpenExtendedMiningChannel.Success (Server -> Client)
/// Sent as a response for opening an extended channel.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OpenExtendedMiningChannelSuccess<'decoder> {
    /// Client-specified request ID from OpenStandardMiningChannel message,
    /// so that the client can pair responses with open channel requests.
    pub request_id: u32,
    /// Newly assigned identifier of the channel, stable for the whole lifetime of
    /// the connection. E.g. it is used for broadcasting new jobs by
    /// NewExtendedMiningJob.
    pub channel_id: u32,
    /// Initial target for the mining channel.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub target: U256<'decoder>,
    /// Extranonce size (in bytes) set for the channel.
    pub extranonce_size: u16,
    /// Bytes used as implicit first part of extranonce
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub extranonce_prefix: B032<'decoder>,
}

/// # OpenMiningChannel.Error (Server -> Client)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenMiningChannelError<'decoder> {
    /// Client-specified request ID from OpenMiningChannel message.
    pub request_id: u32,
    /// Human-readable error code(s).
    /// Possible error codes:
    /// * ‘unknown-user’
    /// * ‘max-target-out-of-range’
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub error_code: Str0255<'decoder>,
}

impl<'a> OpenMiningChannelError<'a> {
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

#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for OpenStandardMiningChannel<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.user_identity.get_size() + 4 + self.max_target.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for OpenMiningChannelError<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size() + self.error_code.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for OpenStandardMiningChannelSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.channel_id.get_size()
            + self.target.get_size()
            + self.extranonce_prefix.get_size()
            + self.group_channel_id.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for OpenExtendedMiningChannel<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.user_identity.get_size()
            + 4
            + self.max_target.get_size()
            + self.min_extranonce_size.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'d> GetSize for OpenExtendedMiningChannelSuccess<'d> {
    fn get_size(&self) -> usize {
        self.request_id.get_size()
            + self.channel_id.get_size()
            + self.target.get_size()
            + self.extranonce_size.get_size()
            + self.extranonce_prefix.get_size()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::tests::from_arbitrary_vec_to_array;
    use alloc::{string::String, vec::Vec};
    use core::convert::TryFrom;
    use quickcheck_macros;

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
            request_id: U32AsRef::from(request_id.clone()),
            user_identity: Str0255::try_from(String::from(user_identity.clone()))
                .expect("could not convert string to Str0255"),
            nominal_hash_rate: nominal_hash_rate.clone(),
            max_target: U256::from(max_target.clone()),
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
            request_id: U32AsRef::from(request_id.clone()),
            channel_id,
            target: U256::from(target.clone()),
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
            request_id: request_id.clone(),
            user_identity: Str0255::try_from(String::from(user_identity.clone()))
                .expect("could not convert string to Str0255"),
            nominal_hash_rate: nominal_hash_rate.clone(),
            max_target: U256::from(max_target.clone()),
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

    #[test]
    fn test() {
        "placeholder to allow in file unit tests for quickcheck";
    }
}

#[cfg(feature = "with_serde")]
impl<'a> OpenExtendedMiningChannel<'a> {
    pub fn into_static(self) -> OpenExtendedMiningChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> OpenExtendedMiningChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> OpenExtendedMiningChannelSuccess<'a> {
    pub fn into_static(self) -> OpenExtendedMiningChannelSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> OpenExtendedMiningChannelSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> OpenMiningChannelError<'a> {
    pub fn into_static(self) -> OpenMiningChannelError<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> OpenMiningChannelError<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> OpenStandardMiningChannel<'a> {
    pub fn into_static(self) -> OpenStandardMiningChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> OpenStandardMiningChannel<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
#[cfg(feature = "with_serde")]
impl<'a> OpenStandardMiningChannelSuccess<'a> {
    pub fn into_static(self) -> OpenStandardMiningChannelSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
    pub fn as_static(&self) -> OpenStandardMiningChannelSuccess<'static> {
        panic!("This function shouldn't be called by the Messaege Generator");
    }
}
