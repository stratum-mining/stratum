/// Performs a round-trip serialization test for a message type.
///
/// This macro:
/// 1. Attempts to parse the input bytes into a message.
/// 2. Serializes the parsed message back into bytes.
/// 3. Parses those bytes again.
/// 4. Re-serializes and checks for byte-level stability.
/// 5. Verifies that the `Display` output is preserved across the round trip.
///
/// # Arguments
///
/// * `$msg_type` — A message type implementing `from_bytes`, `to_bytes`,
///   `get_size` and `Display`.
/// * `$data` — A byte buffer used as the initial source.
///
/// ```ignore
/// test_roundtrip!(MyMessage, input_bytes);
/// ```
#[macro_export]
macro_rules! test_roundtrip {
    ($msg_type:ty, $data:expr) => {{
        // Step 1: Try to parse the input bytes.
        // Invalid inputs are expected in fuzzing, so we silently ignore failures.
        let mut input = $data.clone();
        if let Ok(parsed) = <$msg_type>::from_bytes(&mut input) {
            // Step 2: Serialize the successfully parsed message.
            let mut encoded_1 = vec![0u8; parsed.get_size()];
            parsed
                .clone()
                .to_bytes(&mut encoded_1)
                .expect("Encoding failed after a successful parse");

            // Step 3: Parse the serialized bytes again.
            let mut encoded_1_clone = encoded_1.clone();
            let reparsed = <$msg_type>::from_bytes(&mut encoded_1_clone)
                .expect("Roundtrip failed: serializer produced invalid bytes");

            // Step 4: Serialize again and ensure byte-level stability.
            let mut encoded_2 = vec![0u8; reparsed.get_size()];
            reparsed
                .clone()
                .to_bytes(&mut encoded_2)
                .expect("Second encoding failed");

            assert_eq!(encoded_1, encoded_2, "Serialization is not stable");

            // Step 5: Verify that the content is preserved.
            //
            // Not all message types implement `Eq`, so we compare their `Display`
            // output instead. If both messages can be parsed successfully and
            // represent the same data, their formatted output should match.
            assert_eq!(
                parsed.to_string(),
                reparsed.to_string(),
                "Display output mismatch"
            );
        };
    }};
}
