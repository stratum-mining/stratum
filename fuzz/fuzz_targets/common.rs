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

#[macro_export]
macro_rules! test_datatype_roundtrip {
    // ---- special rule for bool ----
    // Bool has a non-canonical encoding in the spec: only the lowest bit is meaningful.
    // Multiple byte values can parse to the same logical bool, so we cannot require a
    // strict byte-for-byte roundtrip. Instead we check semantic stability and canonicalization.
    (bool, $data:expr) => {{
        let mut input = $data.clone();

        // Only run the roundtrip checks if parsing succeeds. Invalid inputs are ignored,
        // because this macro validates stability of valid encodings, not rejection behavior.
        if let Ok(parsed) = bool::from_bytes(&mut input) {
            // Allocate exactly the number of bytes required by the parsed value.
            // This ensures we test the canonical serialized size.
            let mut encoded = vec![0u8; parsed.get_size()];

            // A successful parse must always be serializable.
            parsed
                .to_bytes(&mut encoded)
                .expect("Bool encoding failed after a successfull parse");

            // Bytes produced by serialization must always be parseable again.
            let reparsed = bool::from_bytes(&mut encoded)
                .expect("The bytes generated from a valid bool should be parseable");

            // Logical value must be preserved by parse → serialize → parse.
            assert_eq!(parsed, reparsed, "Bool roundtrip is not stable");

            // Because only the lowest bit is significant, we compare the semantic bit,
            // not the full original byte. This verifies canonical encoding.
            assert_eq!(input[0] & 1, encoded[0], "Bool serialization is not stable");
        }
    }};

    // ---- special rule for f32 ----
    // Floats require bit-level comparison IEEE-754.
    (f32, $data:expr) => {{
        let mut input = $data.clone();

        // Only validate successful parses; invalid encodings are outside this macro’s scope.
        if let Ok(parsed) = f32::from_bytes(&mut input) {
            // Allocate the exact canonical size of the float representation.
            let mut encoded = vec![0u8; parsed.get_size()];

            // A successfully parsed float must serialize without failure.
            parsed
                .to_bytes(&mut encoded)
                .expect("Encoding failed after a successful parse");

            // Serialized bytes must be parseable back into a float.
            let reparsed = f32::from_bytes(&mut encoded)
                .expect("The bytes generated from a valid datatype should be parseable");

            // Compare raw bits to enforce strict roundtrip stability, including NaN payloads.
            assert_eq!(
                parsed.to_bits(),
                reparsed.to_bits(),
                "Float roundtrip is not bit-stable"
            );

            // Ensure serialization is canonical: re-encoding must match the consumed input.
            assert_eq!(
                encoded,
                input[..encoded.get_size()],
                "Serialization is not stable"
            );
        }
    }};

    // ---- generic rule ----
    ($datatype:ty, $data:expr) => {{
        let mut input = $data.clone();

        // Only test successful parses; this macro checks roundtrip invariants.
        if let Ok(parsed) = <$datatype>::from_bytes(&mut input) {
            // Allocate exactly the canonical serialized size.
            let mut encoded = vec![0u8; parsed.get_size()];

            // A parsed value must always serialize successfully.
            parsed.clone().to_bytes(&mut encoded).expect(concat!(
                stringify!($datatype),
                ": Encoding failed after a successful parse"
            ));

            // Serialized bytes must be parseable again into the same datatype.
            let reparsed = <$datatype>::from_bytes(&mut encoded).expect(concat!(
                stringify!($datatype),
                ": The bytes generated from a valid datatype should be parseable"
            ));

            // Semantic equality after roundtrip is required.
            assert_eq!(
                parsed,
                reparsed,
                "{}: The roundtrip should produce the same message",
                stringify!($datatype)
            );

            // reserialization must match the consumed input bytes.
            assert_eq!(
                encoded,
                input[..encoded.get_size()],
                "{}: Serialization is not stable",
                stringify!($datatype)
            );
        }
    }};
}
