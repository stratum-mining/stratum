//! HTTP transport modules.

#[cfg(feature = "simple_http")]
pub mod simple_http;

/// The default TCP port to use for connections.
/// Set to 8332, the default RPC port for bitcoind.
pub const DEFAULT_PORT: u16 = 8332;

/// The Default SOCKS5 Port to use for proxy connection.
/// Set to 9050, the default RPC port for tor.
// Currently only used by `simple_http` module, here for consistency.
#[cfg(feature = "proxy")]
pub const DEFAULT_PROXY_PORT: u16 = 9050;
