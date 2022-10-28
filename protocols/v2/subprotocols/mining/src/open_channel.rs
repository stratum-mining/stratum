#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::{binary_codec_sv2, U32AsRef};
use binary_sv2::{Deserialize, Serialize, Str0255, B032, U256};
#[cfg(not(feature = "with_serde"))]
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
    pub fn update_id(&mut self, new_id: u32) {
        // DO NOT USE MEM SWAP HERE AS IT DO NOT UPDATE THE UNDERLING PAYLOAD
        // INSTEAD IMPLEMENT U32ASREF FOR SERDE
        todo!()
    }
}

impl<'decoder> OpenStandardMiningChannel<'decoder> {
    pub fn into_static_self(
        s: OpenStandardMiningChannel<'decoder>,
    ) -> OpenStandardMiningChannel<'static> {
        OpenStandardMiningChannel {
            #[cfg(not(feature = "with_serde"))]
            request_id: s.request_id.into_static(),
            #[cfg(feature = "with_serde")]
            request_id: s.request_id,
            user_identity: s.user_identity.into_static(),
            nominal_hash_rate: s.nominal_hash_rate,
            max_target: s.max_target.into_static(),
        }
    }
}

/// # OpenStandardMiningChannel.Success (Server -> Client)
/// Sent as a response for opening a standard channel, if successful.
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
    pub fn update_id(&mut self, new_id: u32) {
        // DO NOT USE MEM SWAP HERE AS IT DO NOT UPDATE THE UNDERLING PAYLOAD
        // INSTEAD IMPLEMENT U32ASREF FOR SERDE
        todo!()
    }
}

/// # OpenExtendedMiningChannel (Client -> Server)
/// Similar to *OpenStandardMiningChannel* but requests to open an extended channel instead of
/// standard channel.
#[derive(Serialize, Deserialize, Debug, Clone)]
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
#[derive(Serialize, Deserialize, Debug, Clone)]
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
