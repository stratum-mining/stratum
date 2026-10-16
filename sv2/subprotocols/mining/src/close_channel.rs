use alloc::{fmt, vec::Vec};
use binary_sv2::{Deserialize, Serialize, Str0255};
use core::convert::TryInto;

/// Message used by a downstream to close a mining channel.
///
/// If you are sending this message through a proxy on behalf of multiple downstreams, you must send
/// it for each open channel separately.
///
/// Upon receiving this message, upstream **must** stop sending messages for the channel.
///
/// Decoding fails if [`CloseChannel::reason_code`] contains control characters, see
/// [`CloseChannel::validate`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[validate]
pub struct CloseChannel<'decoder> {
    /// Channel id of the channel to be closed.
    pub channel_id: u32,
    /// Reason for closing the channel.
    pub reason_code: Str0255<'decoder>,
}

impl fmt::Display for CloseChannel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CloseChannel(channel_id: {}, reason_code: {})",
            self.channel_id,
            self.reason_code.as_utf8_or_hex()
        )
    }
}

impl fmt::Display for CloseChannelOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CloseChannel(channel_id: {}, reason_code: {})",
            self.channel_id,
            self.reason_code.as_utf8_or_hex()
        )
    }
}

impl CloseChannel<'_> {
    /// Check that [`CloseChannel::reason_code`] contains no control characters.
    ///
    /// The spec forbids control characters in error codes and in the other human-readable string
    /// codes such as this one. The `Decodable` derive runs this on every decoded
    /// [`CloseChannel`], so a value carrying one fails at parse time with
    /// [`binary_sv2::Error::ControlCharInStringCode`].
    pub fn validate(&self) -> Result<(), binary_sv2::Error> {
        if let Some(byte) = self.reason_code.first_control_char() {
            return Err(binary_sv2::Error::ControlCharInStringCode(byte));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::{vec, vec::Vec};
    use binary_sv2::GetSize;

    fn parse_close_channel(reason_code: Vec<u8>) -> Result<(), binary_sv2::Error> {
        let msg = CloseChannel {
            channel_id: 0,
            reason_code: Str0255::new(&reason_code).unwrap(),
        };
        let mut bytes = vec![0u8; msg.get_size()];
        msg.clone().to_bytes(&mut bytes).unwrap();
        CloseChannel::from_bytes(&mut bytes).map(|_| ())
    }

    #[test]
    fn parses_a_reason_code_without_control_chars() {
        assert!(parse_close_channel(b"job-ended".to_vec()).is_ok());
    }

    #[test]
    fn rejects_a_reason_code_with_control_chars() {
        for byte in [0x00_u8, 0x0A, 0x1F, 0x7F] {
            assert_eq!(
                parse_close_channel(vec![b'a', byte]),
                Err(binary_sv2::Error::ControlCharInStringCode(byte)),
                "0x{byte:02X} must be rejected"
            );
        }
    }
}
