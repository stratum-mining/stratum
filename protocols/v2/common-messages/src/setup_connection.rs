use serde::{Serialize, Deserialize};
use serde_sv2::{U8, U16, U32, Str0255};

/// ## SetupConnection (Client -> Server)
/// Initiates the connection. This MUST be the first message sent by the client on the newly
/// opened connection. Server MUST respond with either a [`SetupConnectionSuccess`] or
/// [`SetupConnectionError`] message. Clients that are not configured to provide telemetry data to
/// the upstream node SHOULD set device_id to 0-length strings. However, they MUST always set
/// vendor to a string describing the manufacturer/developer and firmware version and SHOULD
/// always set hardware_version to a string describing, at least, the particular hardware/software
/// package in use.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetupConnection {
    /// 0 = Mining Protocol
    /// 1 = Job Negotiation Protocol
    /// 2 = Template Distribution Protocol
    /// 3 = Job Distribution Protocol
    prtocol: U8,
    /// The minimum protocol version the client supports (currently must be 2).
    min_version: U16,
    /// The maximum protocol version the client supports (currently must be 2).
    max_version: U16,
    /// Flags indicating optional protocol features the client supports. Each
    /// protocol from [`SetupConnection.protocol`] field has its own values/flags.
    flags: U32,
    /// ASCII text indicating the hostname or IP address.
    endpoint_host: Str0255,
    /// Connecting port value
    endpoint_port: U16,
    //-- DEVICE INFORMATION --//
    vendor: Str0255,
    hardware_version: Str0255,
    firmware: Str0255,
    device_id: Str0255,
}


/// ## SetupConnection.Success (Server -> Client)
/// Response to [`SetupConnection`] message if the server accepts the connection. The client is
/// required to verify the set of feature flags that the server supports and act accordingly.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetupConnectionSuccess {
    /// Selected version proposed by the connecting node that the upstream
    /// node supports. This version will be used on the connection for the rest
    /// of its life.
    used_version: U16,
    /// Flags indicating optional protocol features the server supports. Each
    /// protocol from [protocol](TODO) field has its own values/flags.
    flags: U32,
}

/// ## SetupConnection.Error (Server -> Client)
/// When protocol version negotiation fails (or there is another reason why the upstream node
/// cannot setup the connection) the server sends this message with a particular error code prior
/// to closing the connection.
/// In order to allow a client to determine the set of available features for a given server (e.g. for
/// proxies which dynamically switch between different pools and need to be aware of supported
/// options), clients SHOULD send a SetupConnection message with all flags set and examine the
/// (potentially) resulting [`SetupConnectionError`] message’s flags field. The Server MUST provide
/// the full set of flags which it does not support in each [`SetupConnectionError`] message and
/// MUST consistently support the same set of flags across all servers on the same hostname and
/// port number. If flags is 0, the error is a result of some condition aside from unsupported flags.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetupConnectionError {
    /// Flags indicating features causing an error.
    flags: U32,
    /// Human-readable error code(s). See Error Codes section, [link](TODO).
    /// ### Possible error codes:
    /// * ‘unsupported-feature-flags’
    /// * ‘unsupported-protocol’
    /// * ‘protocol-version-mismatch’
    error_code: Str0255,
}
