use crate::{
    SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT, SV2_MINING_PROTOCOL_DISCRIMINANT,
    SV2_TEMPLATE_DISTRIBUTION_PROTOCOL_DISCRIMINANT,
};
use alloc::{fmt, vec::Vec};
use binary_sv2::{
    binary_codec_sv2,
    decodable::{DecodableField, FieldMarker},
    Deserialize, GetSize, Serialize, Str0255,
};
use core::convert::{TryFrom, TryInto};

/// Used by downstream to initiate a Stratum V2 connection with an upstream role.
///
/// This is usually the first message sent by a downstream role on a newly opened connection,
/// after completing the handshake process.
///
/// Downstreams that do not wish to provide telemetry data to the upstream role **should** set
/// [`SetupConnection::device_id`] to an empty string. However, they **must** set
/// [`SetupConnection::vendor`] to a string describing the manufacturer/developer and firmware
/// version and **should** set [`SetupConnection::hardware_version`] to a string describing, at
/// least, the particular hardware/software package in use.
///
/// A valid response to this message from the upstream role can either be [`SetupConnectionSuccess`]
/// or [`SetupConnectionError`] message.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SetupConnection<'decoder> {
    /// Protocol to be used for the connection.
    pub protocol: Protocol,
    /// The minimum protocol version supported.
    ///
    /// Currently must be set to 2.
    pub min_version: u16,
    /// The maximum protocol version supported.
    ///
    /// Currently must be set to 2.
    pub max_version: u16,
    /// Flags indicating optional protocol features supported by the downstream.
    ///
    /// Each [`SetupConnection::protocol`] value has it's own flags.
    pub flags: u32,
    /// ASCII representation of the connection hostname or IP address.
    pub endpoint_host: Str0255<'decoder>,
    /// Connection port value.
    pub endpoint_port: u16,
    /// Device vendor name.
    pub vendor: Str0255<'decoder>,
    /// Device hardware version.
    pub hardware_version: Str0255<'decoder>,
    /// Device firmware version.
    pub firmware: Str0255<'decoder>,
    /// Device identifier.
    pub device_id: Str0255<'decoder>,
}

impl fmt::Display for SetupConnection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetupConnection(protocol: {}, min_version: {}, max_version: {}, flags: 0x{:08x}, endpoint_host: {}, endpoint_port: {}, vendor: {}, hardware_version: {}, firmware: {}, device_id: {})",
            self.protocol as u8,
            self.min_version,
            self.max_version,
            self.flags,
            self.endpoint_host.as_utf8_or_hex(),
            self.endpoint_port,
            self.vendor.as_utf8_or_hex(),
            self.hardware_version.as_utf8_or_hex(),
            self.firmware.as_utf8_or_hex(),
            self.device_id.as_utf8_or_hex()
        )
    }
}

impl SetupConnection<'_> {
    /// Set the flag to indicate that the downstream requires a standard job
    pub fn set_requires_standard_job(&mut self) {
        self.flags |= 0b_0000_0000_0000_0000_0000_0000_0000_0001;
    }

    /// Set a flag to indicate that the JDC allows [`Full Template`] mode.
    ///
    /// [`Full Template`]: https://github.com/stratum-mining/sv2-spec/blob/main/06-Job-Declaration-Protocol.md#632-full-template-mode
    pub fn allow_full_template_mode(&mut self) {
        self.flags |= 0b_0000_0000_0000_0000_0000_0000_0000_0001;
    }

    /// Check if passed flags support self flag
    pub fn check_flags(protocol: Protocol, available_flags: u32, required_flags: u32) -> bool {
        match protocol {
            // [0] [0] -> true
            // [0] [1] -> false
            // [1] [1] -> true
            // [0] [1] -> false
            Protocol::MiningProtocol => {
                // Evaluates protocol requirements based on flag bits.
                //
                // Checks if the current protocol meets the required flags for work selection and
                // version rolling by reversing the bits of `available_flags` and
                // `required_flags`. It extracts the 30th and 29th bits to determine
                // if work selection and version rolling are needed.
                //
                // Returns `true` if:
                // - The work selection requirement is satisfied or not needed.
                // - The version rolling requirement is satisfied or not needed.
                //
                // Otherwise, returns `false`.
                let available = available_flags.reverse_bits();
                let required_flags = required_flags.reverse_bits();
                let requires_work_selection_passed = required_flags >> 30 > 0;
                let requires_version_rolling_passed = required_flags >> 29 > 0;

                let requires_work_selection_self = available >> 30 > 0;
                let requires_version_rolling_self = available >> 29 > 0;

                let work_selection =
                    !requires_work_selection_self || requires_work_selection_passed;
                let version_rolling =
                    !requires_version_rolling_self || requires_version_rolling_passed;

                work_selection && version_rolling
            }
            Protocol::JobDeclarationProtocol => {
                // Determines if asynchronous job mining is required based on flag bits.
                //
                // Reverses the bits of `available_flags` and `required_flags`, extracts the 31st
                // bit from each, and evaluates if the condition is met using these
                // bits. Returns `true` or `false` based on:
                // - False if `full_template_mode_required_self` is true and
                //   `allow_full_teamplate_mode_passed` is false.
                // - True otherwise.
                //
                // TODO: Simplify this code by removing the `reverse_bits` function and
                // using bitwise operations directly. i.e., try to use `&` and `|` to
                // combine the bits instead of reversing them.
                let available = available_flags.reverse_bits();
                let required = required_flags.reverse_bits();

                let allow_full_teamplate_mode_passed = (required >> 31) & 1 > 0;
                let full_template_mode_required_self = (available >> 31) & 1 > 0;

                match (
                    full_template_mode_required_self,
                    allow_full_teamplate_mode_passed,
                ) {
                    (true, true) => true,
                    (true, false) => false,
                    (false, true) => true,
                    (false, false) => true,
                }
            }
            Protocol::TemplateDistributionProtocol => {
                // These protocols do not define flags for setting up a connection.
                false
            }
        }
    }

    /// Check whether received versions are supported.
    ///
    /// If the versions are not supported, return `None` otherwise return the biggest version
    /// available
    pub fn get_version(&self, min_version: u16, max_version: u16) -> Option<u16> {
        if self.min_version > max_version || min_version > self.max_version {
            None
        } else {
            Some(self.max_version.min(max_version))
        }
    }

    /// Checks whether passed flags indicate that the downstream requires standard job.
    pub fn requires_standard_job(&self) -> bool {
        has_requires_std_job(self.flags)
    }
}

/// Helper function to check if `REQUIRES_STANDARD_JOBS` bit flag present.
pub fn has_requires_std_job(flags: u32) -> bool {
    let flags = flags.reverse_bits();
    let flag = flags >> 31;
    flag != 0
}

/// Helper function to check if `REQUIRES_VERSION_ROLLING` bit flag present.
pub fn has_version_rolling(flags: u32) -> bool {
    let flags = flags.reverse_bits();
    let flags = flags << 2;
    let flag = flags >> 31;
    flag != 0
}

/// Helper function to check if `REQUIRES_WORK_SELECTION` bit flag present.
pub fn has_work_selection(flags: u32) -> bool {
    let flags = flags.reverse_bits();
    let flags = flags << 1;
    let flag = flags >> 31;
    flag != 0
}

/// Message used by an upstream role to accept a connection setup request from a downstream role.
///
/// This message is sent in response to a [`SetupConnection`] message.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]

pub struct SetupConnectionSuccess {
    /// Selected version based on the [`SetupConnection::min_version`] and
    /// [`SetupConnection::max_version`] sent by the downstream role.
    ///
    /// This version will be used on the connection for the rest of its life.
    pub used_version: u16,
    /// Flags indicating optional protocol features supported by the upstream.
    ///
    /// The downstream is required to verify this set of flags and act accordingly.
    ///
    /// Each [`SetupConnection::protocol`] field has its own values/flags.
    pub flags: u32,
}

impl fmt::Display for SetupConnectionSuccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetupConnectionSuccess(used_version: {}, flags: 0x{:08x})",
            self.used_version, self.flags
        )
    }
}

/// Message used by an upstream role to reject a connection setup request from a downstream role.
///
/// This message is sent in response to a [`SetupConnection`] message.
///
/// The connection setup process could fail because of protocol version negotiation. In order to
/// allow a downstream to determine the set of available features for a given upstream (e.g. for
/// proxies which dynamically switch between different pools and need to be aware of supported
/// options), downstream should send a [`SetupConnection`] message with all flags set and examine
/// the (potentially) resulting [`SetupConnectionError`] message’s flags field.
///
/// The upstream must provide the full set of flags which it does not support in each
/// [`SetupConnectionError`] message and must consistently support the same set of flags across all
/// servers on the same hostname and port number.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SetupConnectionError<'decoder> {
    /// Unsupported feature flags.
    ///
    /// In case `error_code` is `unsupported-feature-flags`, this field is used to indicate which
    /// flag is causing an error, otherwise it will be set to 0.
    pub flags: u32,
    /// Reason for setup connection error.
    ///
    /// Possible error codes:
    /// - unsupported-feature-flags
    /// - unsupported-protocol
    /// - protocol-version-mismatch
    pub error_code: Str0255<'decoder>,
}

impl fmt::Display for SetupConnectionError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SetupConnectionError(flags: 0x{:08x}, error_code: {})",
            self.flags,
            self.error_code.as_utf8_or_hex()
        )
    }
}

/// This enum has a list of the different Stratum V2 subprotocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum Protocol {
    /// Mining protocol.
    MiningProtocol = SV2_MINING_PROTOCOL_DISCRIMINANT,
    /// Job declaration protocol.
    JobDeclarationProtocol = SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT,
    /// Template distribution protocol.
    TemplateDistributionProtocol = SV2_TEMPLATE_DISTRIBUTION_PROTOCOL_DISCRIMINANT,
}

impl From<Protocol> for binary_sv2::encodable::EncodableField<'_> {
    fn from(v: Protocol) -> Self {
        let val = v as u8;
        val.into()
    }
}

impl<'decoder> binary_sv2::Decodable<'decoder> for Protocol {
    fn get_structure(
        _: &[u8],
    ) -> core::result::Result<alloc::vec::Vec<FieldMarker>, binary_sv2::Error> {
        let field: FieldMarker = (0_u8).into();
        Ok(alloc::vec![field])
    }
    fn from_decoded_fields(
        mut v: alloc::vec::Vec<DecodableField<'decoder>>,
    ) -> core::result::Result<Self, binary_sv2::Error> {
        let val = v.pop().ok_or(binary_sv2::Error::NoDecodableFieldPassed)?;
        let val: u8 = val.try_into()?;
        val.try_into()
            .map_err(|_| binary_sv2::Error::ValueIsNotAValidProtocol(val))
    }
}

impl TryFrom<u8> for Protocol {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            SV2_MINING_PROTOCOL_DISCRIMINANT => Ok(Protocol::MiningProtocol),
            SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT => Ok(Protocol::JobDeclarationProtocol),
            SV2_TEMPLATE_DISTRIBUTION_PROTOCOL_DISCRIMINANT => {
                Ok(Protocol::TemplateDistributionProtocol)
            }
            _ => Err(()),
        }
    }
}

impl GetSize for Protocol {
    fn get_size(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::alloc::string::ToString;
    use core::convert::TryInto;

    #[test]
    fn test_check_flag() {
        let protocol = crate::Protocol::MiningProtocol;
        let flag_available = 0b_0000_0000_0000_0000_0000_0000_0000_0000;
        let flag_required = 0b_0000_0000_0000_0000_0000_0000_0000_0001;
        assert!(SetupConnection::check_flags(
            protocol,
            flag_available,
            flag_required
        ));

        let protocol = crate::Protocol::JobDeclarationProtocol;

        let available_flags = 0b_1000_0000_0000_0000_0000_0000_0000_0000;
        let required_flags = 0b_1000_0000_0000_0000_0000_0000_0000_0000;
        assert!(SetupConnection::check_flags(
            protocol,
            available_flags,
            required_flags
        ));
    }

    #[test]
    fn test_has_requires_std_job() {
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0001;
        assert!(has_requires_std_job(flags));
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0010;
        assert!(!has_requires_std_job(flags));
    }

    #[test]
    fn test_has_version_rolling() {
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0100;
        assert!(has_version_rolling(flags));
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0001;
        assert!(!has_version_rolling(flags));
    }

    #[test]
    fn test_has_work_selection() {
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0010;
        assert!(has_work_selection(flags));
        let flags = 0b_0000_0000_0000_0000_0000_0000_0000_0001;
        assert!(!has_work_selection(flags));
    }

    fn create_setup_connection() -> SetupConnection<'static> {
        SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version: 1,
            max_version: 4,
            flags: 0,
            endpoint_host: "0.0.0.0".to_string().into_bytes().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "vendor".to_string().into_bytes().try_into().unwrap(),
            hardware_version: "hw_version".to_string().into_bytes().try_into().unwrap(),
            firmware: "firmware".to_string().into_bytes().try_into().unwrap(),
            device_id: "device_id".to_string().into_bytes().try_into().unwrap(),
        }
    }

    #[test]
    fn test_get_version() {
        let setup_conn = create_setup_connection();
        assert_eq!(setup_conn.get_version(1, 5).unwrap(), 4);
        assert_eq!(setup_conn.get_version(6, 6), None);
    }

    // Test SetupConnection::set_requires_std_job
    #[test]
    fn test_set_requires_std_job() {
        let mut setup_conn = create_setup_connection();
        assert!(!setup_conn.requires_standard_job());
        setup_conn.set_requires_standard_job();
        assert!(setup_conn.requires_standard_job());
    }
}
