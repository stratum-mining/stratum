#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::binary_codec_sv2;
use binary_sv2::{Deserialize, Serialize, Str0255};
#[cfg(not(feature = "with_serde"))]
use core::convert::TryInto;

/// # Reconnect (Server -> Client)
///
/// This message allows clients to be redirected to a new upstream node.
///
/// This message is connection-related so that it should not be propagated downstream by
/// intermediate proxies. Upon receiving the message, the client re-initiates the Noise handshake
/// and uses the pool’s authority public key to verify that the certificate presented by the new
/// server has a valid signature.
///
/// For security reasons, it is not possible to reconnect to a server with a certificate signed by a
/// different pool authority key. The message intentionally does *not* contain a **pool public key**
/// and thus cannot be used to reconnect to a different pool. This ensures that an attacker will not
/// be able to redirect hashrate to an arbitrary server should the pool server get compromised and
/// instructed to send reconnects to a new location.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reconnect<'decoder> {
    /// When empty, downstream node attempts to reconnect to its present
    /// host.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    pub new_host: Str0255<'decoder>,
    /// When 0, downstream node attempts to reconnect to its present port.
    pub new_port: u16,
}
#[cfg(feature = "with_serde")]
use binary_sv2::GetSize;
#[cfg(feature = "with_serde")]
impl<'d> GetSize for Reconnect<'d> {
    fn get_size(&self) -> usize {
        self.new_host.get_size() + self.new_port.get_size()
    }
}
#[cfg(feature = "with_serde")]
impl<'a> Reconnect<'a> {
    pub fn into_static(self) -> Reconnect<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
    pub fn as_static(&self) -> Reconnect<'static> {
        panic!("This function shouldn't be called by the Message Generator");
    }
}
