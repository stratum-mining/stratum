use alloc::{fmt, vec::Vec};
use binary_sv2::{binary_codec_sv2, Deserialize, Serialize, Str0255};
use core::convert::TryInto;

/// Message used by upstream to redirect downstream connection(s) to a new host.
///
/// Upon receiving the message, the downstream re-initiates the Noise Handshake process and uses
/// the pool’s authority public key to verify the certificate presented by the new server.
///
/// For security reasons, it is not possible to reconnect to an upstream with a certificate signed
/// by a different pool authority key. The message intentionally does not contain a pool public key
/// and thus cannot be used to reconnect to a different pool. This ensures that an attacker will
/// not be able to redirect hashrate to an arbitrary server in case the pool server get compromised
/// and instructed to send reconnects to a new location.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reconnect<'decoder> {
    /// When empty, downstream node should attempt to reconnect to current pool host.
    pub new_host: Str0255<'decoder>,
    /// When 0, downstream node should attempt to reconnect to current pool host.
    pub new_port: u16,
}

impl fmt::Display for Reconnect<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Reconnect(new_host: {}, new_port: {})",
            self.new_host.as_utf8_or_hex(),
            self.new_port
        )
    }
}

impl PartialEq for Reconnect<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.new_host.as_ref() == other.new_host.as_ref() && self.new_port == other.new_port
    }
}
