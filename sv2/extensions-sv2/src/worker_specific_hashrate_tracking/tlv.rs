use alloc::vec::Vec;
use binary_sv2::to_bytes;
use codec_sv2::StandardSv2Frame;
use framing_sv2::header::Header;
use mining_sv2::{
    SubmitSharesExtended, CHANNEL_BIT_SUBMIT_SHARES_EXTENDED, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
};

use super::{UserIdentity, EXTENSION_TYPE, FIELD_TYPE_USER_IDENTITY, MAX_WORKER_ID_LENGTH};
use crate::tlv::{self, TlvError};

/// Encodes a UserIdentity as a TLV field for appending to SubmitSharesExtended.
///
/// Uses the generic TLV encoder with Worker Hashrate Tracking extension-specific
/// parameters (extension_type=0x0002, field_type=0x01).
pub fn encode_user_identity_as_tlv_bytes(
    user_identity: &UserIdentity,
) -> Result<Vec<u8>, TlvError> {
    let worker_bytes = user_identity.as_bytes();

    // Enforce 32-byte maximum as per spec
    if worker_bytes.len() > MAX_WORKER_ID_LENGTH {
        return Err(TlvError::WorkerIdTooLong(worker_bytes.len()));
    }

    // Use generic TLV encoder
    let tlv = tlv::Tlv::new(
        EXTENSION_TYPE,
        FIELD_TYPE_USER_IDENTITY,
        worker_bytes.to_vec(),
    );
    tlv.encode()
}

/// Decodes a UserIdentity from TLV bytes.
///
/// Uses the generic TLV decoder and validates that the extension and field types
/// match the Worker Hashrate Tracking specification.
pub fn decode_user_identity_from_tlv_bytes(data: &[u8]) -> Result<UserIdentity, TlvError> {
    // Use generic TLV decoder
    let tlv = tlv::Tlv::decode(data)?;

    // Verify extension type
    if tlv.extension_type != EXTENSION_TYPE {
        return Err(TlvError::InvalidExtensionType(tlv.extension_type));
    }

    // Verify field type
    if tlv.field_type != FIELD_TYPE_USER_IDENTITY {
        return Err(TlvError::InvalidFieldType(tlv.field_type));
    }

    // Enforce 32-byte maximum
    if tlv.value.len() > MAX_WORKER_ID_LENGTH {
        return Err(TlvError::WorkerIdTooLong(tlv.value.len()));
    }

    // Create UserIdentity from raw bytes
    let user_identity = UserIdentity {
        user_identity: tlv.value,
    };

    Ok(user_identity)
}

// /// Extracts worker identity from a `SubmitSharesExtended` message and its raw payload.
// ///
// /// This is a convenience function for upstream applications (like pools) to extract
// /// worker identity from TLV-extended `SubmitSharesExtended` messages.
// ///
// /// The function automatically calculates the base message size from the deserialized
// /// message, then extracts any TLV data that follows.
// ///
// /// # Arguments
// /// * `message` - The deserialized `SubmitSharesExtended` message
// /// * `frame_payload` - The complete raw frame payload (base message + optional TLV)
// ///
// /// # Returns
// /// * `Ok(Some(UserIdentity))` - If TLV worker identity is present and valid
// /// * `Ok(None)` - If no TLV data is present or extension type doesn't match
// /// * `Err(TlvError)` - If TLV data is present but invalid
// ///
// /// # Example
// ///
// /// ```ignore
// /// use extensions_sv2::extract_worker_identity_from_submit_shares;
// /// use binary_sv2::from_bytes;
// /// use mining_sv2::SubmitSharesExtended;
// ///
// /// // After receiving a frame with SubmitSharesExtended
// /// let payload = frame.payload();
// /// let mut payload_copy = payload.to_vec();
// /// let base_msg: SubmitSharesExtended = from_bytes(&mut payload_copy)?;
// ///
// /// // Extract worker identity if present
// /// match extract_worker_identity_from_submit_shares(&base_msg, payload)? {
// ///     Some(worker_id) => {
// ///         println!("Share from worker: {}", worker_id.as_string_or_hex());
// ///     }
// ///     None => {
// ///         println!("Share without worker identity");
// ///     }
// /// }
// /// ```
// pub fn extract_worker_identity_from_submit_shares<'a>(
//     message: &mining_sv2::SubmitSharesExtended<'a>,
//     frame_payload: &[u8],
// ) -> Result<Option<UserIdentity>, TlvError> {
//     use binary_sv2::GetSize;
//
//     // Calculate the base message size from the deserialized message
//     let base_message_size = message.get_size();
//
//     // Check if there's any data after the base message
//     if frame_payload.len() <= base_message_size {
//         return Ok(None);
//     }
//
//     // Get the TLV data portion
//     let tlv_data = &frame_payload[base_message_size..];
//
//     // Check if it's valid TLV for this extension
//     if !tlv::has_tlv_for_extension(tlv_data, EXTENSION_TYPE) {
//         return Ok(None);
//     }
//
//     // Decode the TLV
//     let user_identity = decode_user_identity_tlv(tlv_data)?;
//
//     Ok(Some(user_identity))
// }

/// Creates a `StandardSv2Frame` for `SubmitSharesExtended` with optional TLV worker identity.
///
/// This is the primary function for creating `SubmitSharesExtended` frames with Worker-Specific
/// Hashrate Tracking extensionsupport.
pub fn build_submit_shares_extended_with_user_identity_frame<'a>(
    message: SubmitSharesExtended<'a>,
    user_identity: &str,
) -> Result<StandardSv2Frame<SubmitSharesExtended<'a>>, TlvError> {
    // Build payload: base message + optional TLV
    let mut payload = to_bytes(message).map_err(TlvError::SerializationFailed)?;

    // Only add TLV if user_identity is not empty
    if !user_identity.is_empty() {
        let identity = UserIdentity::new(user_identity)
            .map_err(|_| TlvError::WorkerIdTooLong(user_identity.len()))?;
        let tlv_bytes = encode_user_identity_as_tlv_bytes(&identity)?;
        payload.extend_from_slice(&tlv_bytes);
    }

    let msg_length = payload.len() as u32;
    let extension_type = if CHANNEL_BIT_SUBMIT_SHARES_EXTENDED {
        1u16 << 15
    } else {
        0
    };

    // Construct complete frame bytes
    let mut frame_bytes = Vec::with_capacity(Header::SIZE + payload.len());
    frame_bytes.extend_from_slice(&extension_type.to_le_bytes());
    frame_bytes.push(MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED);
    frame_bytes.push((msg_length & 0xFF) as u8);
    frame_bytes.push(((msg_length >> 8) & 0xFF) as u8);
    frame_bytes.push(((msg_length >> 16) & 0xFF) as u8);
    frame_bytes.extend_from_slice(&payload);

    // Convert to StandardSv2Frame
    StandardSv2Frame::from_bytes(frame_bytes.into())
        .map_err(|_| TlvError::SerializationFailed(binary_sv2::Error::WriteError(0, 0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    //  use binary_sv2::GetSize;

    #[test]
    fn test_encode_decode_user_identity_tlv() {
        let original = UserIdentity::new("Worker_001").unwrap();

        let tlv_bytes = encode_user_identity_as_tlv_bytes(&original).unwrap();

        // Verify TLV structure (little-endian via binary_sv2)
        assert_eq!(tlv_bytes[0], 0x02); // extension_type low byte (0x0002)
        assert_eq!(tlv_bytes[1], 0x00); // extension_type high byte
        assert_eq!(tlv_bytes[2], 0x01); // field_type (0x01)
        assert_eq!(tlv_bytes[3], 0x0a); // length low byte (10 bytes)
        assert_eq!(tlv_bytes[4], 0x00); // length high byte

        // Decode and verify
        let decoded = decode_user_identity_from_tlv_bytes(&tlv_bytes).unwrap();
        assert_eq!(decoded.as_str().unwrap(), "Worker_001");
    }

    #[test]
    fn test_decode_invalid_extension_type() {
        let mut tlv_bytes = Vec::new();
        tlv_bytes.extend_from_slice(&[0x99, 0x00]); // Wrong extension type (little-endian 0x0099)
        tlv_bytes.push(0x01); // field_type
        tlv_bytes.extend_from_slice(&[0x04, 0x00]); // length (4 bytes for "test", little-endian)
        tlv_bytes.extend_from_slice(b"test");

        let result = decode_user_identity_from_tlv_bytes(&tlv_bytes);
        assert!(matches!(
            result,
            Err(TlvError::InvalidExtensionType(0x0099))
        ));
    }

    #[test]
    fn test_decode_invalid_field_type() {
        let mut tlv_bytes = Vec::new();
        tlv_bytes.extend_from_slice(&[0x02, 0x00]); // Correct extension type (little-endian 0x0002)
        tlv_bytes.push(0x99); // Wrong field_type
        tlv_bytes.extend_from_slice(&[0x04, 0x00]); // length (4 bytes for "test", little-endian)
        tlv_bytes.extend_from_slice(b"test");

        let result = decode_user_identity_from_tlv_bytes(&tlv_bytes);
        assert!(matches!(result, Err(TlvError::InvalidFieldType(0x99))));
    }

    #[test]
    fn test_encode_exceeds_max_length() {
        let too_long = "x".repeat(33);
        let user_identity = UserIdentity::new(&too_long).unwrap_or(UserIdentity {
            user_identity: too_long.into_bytes(),
        });

        let result = encode_user_identity_as_tlv_bytes(&user_identity);
        assert!(matches!(result, Err(TlvError::WorkerIdTooLong(33))));
    }

    #[test]
    fn test_encode_exactly_32_bytes() {
        let exactly_32 = "Worker_1234567890123456789012345";
        assert_eq!(exactly_32.len(), MAX_WORKER_ID_LENGTH);

        let user_identity = UserIdentity::new(exactly_32).unwrap();

        let result = encode_user_identity_as_tlv_bytes(&user_identity);
        assert!(result.is_ok());
    }

    // #[test]
    // fn test_extract_worker_identity_from_submit_shares() {
    //     use binary_sv2::to_bytes;
    //     use mining_sv2::SubmitSharesExtended;
    //
    //     let base_msg = SubmitSharesExtended {
    //         channel_id: 1,
    //         sequence_number: 42,
    //         job_id: 1,
    //         nonce: 0x12345678,
    //         ntime: 0x87654321,
    //         version: 0x20000000,
    //         extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
    //     };
    //
    //     let mut payload = to_bytes(base_msg.clone()).unwrap();
    //
    //     // Test 1: No TLV data
    //     let result = extract_worker_identity_from_submit_shares(&base_msg, &payload).unwrap();
    //     assert!(result.is_none());
    //
    //     // Test 2: With TLV data
    //     let worker_id = UserIdentity::new("Worker_001").unwrap();
    //     let tlv_bytes = encode_user_identity_as_tlv_bytes(&worker_id).unwrap();
    //     payload.extend_from_slice(&tlv_bytes);
    //
    //     let result = extract_worker_identity_from_submit_shares(&base_msg, &payload).unwrap();
    //     assert!(result.is_some());
    //     assert_eq!(result.unwrap().as_str().unwrap(), "Worker_001");
    // }

    // #[test]
    // fn test_extract_worker_identity_invalid_tlv() {
    //     use binary_sv2::to_bytes;
    //     use mining_sv2::SubmitSharesExtended;
    //
    //     let base_msg = SubmitSharesExtended {
    //         channel_id: 1,
    //         sequence_number: 42,
    //         job_id: 1,
    //         nonce: 0x12345678,
    //         ntime: 0x87654321,
    //         version: 0x20000000,
    //         extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
    //     };
    //
    //     let mut payload = to_bytes(base_msg.clone()).unwrap();
    //
    //     // Append invalid TLV (wrong extension type)
    //     payload.extend_from_slice(&[0x00, 0x99, 0x01, 0x00, 0x05]);
    //
    //     // Should return None because the extension type doesn't match
    //     let result = extract_worker_identity_from_submit_shares(&base_msg, &payload).unwrap();
    //     assert!(result.is_none());
    // }

    fn create_test_submit_shares_extended() -> SubmitSharesExtended<'static> {
        SubmitSharesExtended {
            channel_id: 1,
            sequence_number: 100,
            job_id: 42,
            nonce: 0x12345678,
            ntime: 1234567890,
            version: 0x20000000,
            extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
        }
    }

    #[test]
    fn test_frame_without_user_identity() {
        let base = create_test_submit_shares_extended();
        let frame = build_submit_shares_extended_with_user_identity_frame(base, "").unwrap();

        // Frame should have content
        assert!(frame.encoded_length() > Header::SIZE);
    }

    #[test]
    fn test_frame_with_user_identity() {
        let base = create_test_submit_shares_extended();

        let frame_with =
            build_submit_shares_extended_with_user_identity_frame(base.clone(), "Worker_001")
                .unwrap();
        let frame_without =
            build_submit_shares_extended_with_user_identity_frame(base, "").unwrap();

        // Frame with TLV should be longer
        // TLV overhead: 5 bytes (header) + 10 bytes ("Worker_001") = 15 bytes
        assert_eq!(
            frame_with.encoded_length(),
            frame_without.encoded_length() + 15
        );
    }

    #[test]
    fn test_user_identity_too_long() {
        let base = create_test_submit_shares_extended();
        let too_long = "x".repeat(33);

        let result = build_submit_shares_extended_with_user_identity_frame(base, &too_long);

        assert!(matches!(result, Err(TlvError::WorkerIdTooLong(33))));
    }

    #[test]
    fn test_user_identity_exactly_32_bytes() {
        let base = create_test_submit_shares_extended();
        let exactly_32 = "Worker_1234567890123456789012345";
        assert_eq!(exactly_32.len(), MAX_WORKER_ID_LENGTH);

        let result = build_submit_shares_extended_with_user_identity_frame(base, exactly_32);

        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_user_identity() {
        let base = create_test_submit_shares_extended();

        // Empty string is valid (0-byte TLV value)
        let result = build_submit_shares_extended_with_user_identity_frame(base, "");

        assert!(result.is_ok());
    }

    // #[test]
    // fn test_frame_deserialization_with_tlv() {
    //     use binary_sv2::from_bytes;
    //
    //     // Simulate client side: Create a frame with worker identity
    //     let base_msg = create_test_submit_shares_extended();
    //     let worker_name = "Worker_001";
    //     let mut frame = build_submit_shares_extended_with_user_identity_frame(
    //         base_msg.clone(),
    //         Some(worker_name),
    //     )
    //     .unwrap();
    //
    //     // Simulate pool/server side: Receive the frame
    //     // Get the raw payload from the frame (this is what handlers receive)
    //     let payload = frame.payload();
    //
    //     // Pool tries to deserialize the message (like handlers do)
    //     let mut payload_for_parsing = payload.to_vec();
    //     let parsed_msg: SubmitSharesExtended = from_bytes(&mut payload_for_parsing)
    //         .expect("Should deserialize successfully despite TLV bytes");
    //
    //     // Verify the base message was parsed correctly
    //     assert_eq!(parsed_msg.get_size(), base_msg.get_size());
    //     assert_eq!(parsed_msg.channel_id, base_msg.channel_id);
    //     assert_eq!(parsed_msg.sequence_number, base_msg.sequence_number);
    //     assert_eq!(parsed_msg.job_id, base_msg.job_id);
    //     assert_eq!(parsed_msg.nonce, base_msg.nonce);
    //
    //     // Now extract the worker identity from the raw payload
    //     let extracted_worker = extract_worker_identity_from_submit_shares(&parsed_msg, payload)
    //         .expect("Should extract worker identity successfully");
    //
    //     assert!(
    //         extracted_worker.is_some(),
    //         "Worker identity should be present"
    //     );
    //     assert_eq!(
    //         extracted_worker.unwrap().as_str().unwrap(),
    //         worker_name,
    //         "Extracted worker name should match original"
    //     );
    // }

    // #[test]
    // fn test_frame_deserialization_without_tlv() {
    //     use binary_sv2::from_bytes;
    //
    //     // Create a frame WITHOUT worker identity
    //     let base_msg = create_test_submit_shares_extended();
    //     let mut frame =
    //         build_submit_shares_extended_with_user_identity_frame(base_msg.clone(), None).unwrap();
    //
    //     // Get the raw payload
    //     let payload = frame.payload();
    //
    //     // Deserialize the message
    //     let mut payload_for_parsing = payload.to_vec();
    //     let parsed_msg: SubmitSharesExtended =
    //         from_bytes(&mut payload_for_parsing).expect("Should deserialize successfully");
    //
    //     // Verify the base message was parsed correctly
    //     assert_eq!(parsed_msg.channel_id, base_msg.channel_id);
    //
    //     // Try to extract worker identity - should be None
    //     let extracted_worker = extract_worker_identity_from_submit_shares(&parsed_msg, payload)
    //         .expect("Should not error when no TLV present");
    //
    //     assert!(
    //         extracted_worker.is_none(),
    //         "Worker identity should be None when no TLV is present"
    //     );
    // }
}
