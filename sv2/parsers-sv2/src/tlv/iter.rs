//! TLV iterator for parsing byte streams.

use super::{Tlv, TlvError, TLV_HEADER_SIZE};

/// Iterator over TLV fields in a byte buffer.
///
/// Lazily decodes TLV fields as the iterator is consumed. Stops iteration
/// on the first decode error or when no more complete TLV headers can be read.
pub struct TlvIter<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> TlvIter<'a> {
    /// Creates a new TLV iterator over the provided data.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = Result<Tlv, TlvError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we have enough data for a TLV header
        if self.offset + TLV_HEADER_SIZE > self.data.len() {
            return None;
        }

        match Tlv::decode(&self.data[self.offset..]) {
            Ok(tlv) => {
                let size = tlv.encoded_size();
                self.offset += size;
                Some(Ok(tlv))
            }
            Err(e) => {
                // Stop iteration on error by advancing to end
                self.offset = self.data.len();
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    extern crate alloc;

    #[test]
    fn test_tlv_iter() {
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let tlv2 = Tlv::new(0x0003, 0x01, b"data".to_vec());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&tlv1.encode().unwrap());
        buffer.extend_from_slice(&tlv2.encode().unwrap());

        let mut iter = TlvIter::new(&buffer);

        assert_eq!(iter.next().unwrap().unwrap(), tlv1);
        assert_eq!(iter.next().unwrap().unwrap(), tlv2);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_tlv_iter_empty() {
        let buffer: Vec<u8> = Vec::new();
        let mut iter = TlvIter::new(&buffer);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_tlv_iter_error_stops_iteration() {
        // Create a buffer with one valid TLV and then corrupt data
        let tlv1 = Tlv::new(0x0002, 0x01, b"worker1".to_vec());
        let mut buffer = tlv1.encode().unwrap();

        // Add incomplete TLV header (only 3 bytes)
        buffer.extend_from_slice(&[0x03, 0x00, 0x01]);

        let mut iter = TlvIter::new(&buffer);

        // First TLV should be valid
        assert_eq!(iter.next().unwrap().unwrap(), tlv1);

        // Second should be None (incomplete header)
        assert!(iter.next().is_none());
    }
}
