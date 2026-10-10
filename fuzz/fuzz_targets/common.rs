use arbitrary::Unstructured;
use serde_json::Value;

/// Round-trip serialization test for a message type.
///
/// Generator mode with validation (4 args) produces valid wire bytes via a generator,
/// runs a spec-constraint validation closure on the parsed message, then asserts
/// byte-level stability and Display output equality.
///
/// Generator mode (3 args) produces valid wire bytes via a generator and
/// asserts byte-level stability and Display output equality.
///
/// Raw bytes mode (2 args) attempts to parse the raw input bytes and
/// asserts byte-level stability and Display output equality on success.
#[macro_export]
macro_rules! test_roundtrip {
    // ---- generator mode ----
    ($msg_type:ty, $data:expr, $gen:expr) => {{
        let mut u = arbitrary::Unstructured::new(&$data);
        if let Ok(bytes) = $gen(&mut u) {
            let mut bytes = bytes;
            let parsed =
                <$msg_type>::from_bytes(&mut bytes).expect("generator produced unparseable bytes");


            let mut encoded_1 = vec![0u8; parsed.get_size()];
            parsed
                .clone()
                .to_bytes(&mut encoded_1)
                .expect("Encoding failed after a successful parse");


            let mut encoded_1_clone = encoded_1.clone();
            let reparsed = <$msg_type>::from_bytes(&mut encoded_1_clone)
                .expect("Roundtrip failed: serializer produced invalid bytes");


            let mut encoded_2 = vec![0u8; reparsed.get_size()];
            reparsed
                .clone()
                .to_bytes(&mut encoded_2)
                .expect("Second encoding failed");


            assert_eq!(encoded_1, encoded_2, "Serialization is not stable");
            assert!(!encoded_1.is_empty(), "Encoded output must not be empty");
            assert_eq!(
                encoded_1.len(),
                parsed.get_size(),
                "Encoded length must match get_size()"
            );
            assert_eq!(
                reparsed.get_size(),
                parsed.get_size(),
                "Roundtrip must preserve get_size()"
            );
            let display = parsed.to_string();
            assert_eq!(
                display,
                reparsed.to_string(),
                "Display output mismatch"
            );

            // Spec 3.4.3: TLV fields MUST be placed at the end of the message payload.
            // Appending trailing bytes after a complete encoding must not change the
            // decoded value or size, because decoders must stop after the base fields.
            let mut with_trailing = encoded_1.clone();
            with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
            let with_trailing_parsed = <$msg_type>::from_bytes(&mut with_trailing)
                .expect(concat!(stringify!($msg_type),
                    ": trailing bytes after complete message must be ignored"));
            assert_eq!(
                with_trailing_parsed.to_string(),
                display,
                "{}: trailing bytes changed decoded message",
                stringify!($msg_type)
            );
            assert_eq!(
                with_trailing_parsed.get_size(),
                parsed.get_size(),
                "{}: trailing bytes changed decoded size",
                stringify!($msg_type)
            );
        }
    }};
    // ---- raw-bytes mode -----
    ($msg_type:ty, $data:expr) => {{
        // Invalid inputs are expected in fuzzing, so ignore failures.
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

            // Not all message types implement Eq, so compare Display output.
            let display = parsed.to_string();
            assert_eq!(
                display,
                reparsed.to_string(),
                "Display output mismatch"
            );

            // Spec 3.4.3: TLV fields MUST be placed at the end of the message payload.
            // Appending trailing bytes after a complete encoding must not change the
            // decoded value or size, because decoders must stop after the base fields.
            let mut with_trailing = encoded_1.clone();
            with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
            let with_trailing_parsed = <$msg_type>::from_bytes(&mut with_trailing)
                .expect(concat!(stringify!($msg_type),
                    ": trailing bytes after complete message must be ignored"));
            assert_eq!(
                with_trailing_parsed.to_string(),
                display,
                "{}: trailing bytes changed decoded message",
                stringify!($msg_type)
            );
            assert_eq!(
                with_trailing_parsed.get_size(),
                parsed.get_size(),
                "{}: trailing bytes changed decoded size",
                stringify!($msg_type)
            );
        };
    }};
    // ---- generator mode with spec validation ----
    ($msg_type:ty, $data:expr, $gen:expr, $validate:expr) => {{
        let mut u = arbitrary::Unstructured::new(&$data);
        if let Ok(bytes) = $gen(&mut u) {
            let mut bytes = bytes;
            if let Ok(parsed) = <$msg_type>::from_bytes(&mut bytes) {
                $validate(&parsed);

                let mut encoded_1 = vec![0u8; parsed.get_size()];
                parsed
                    .clone()
                    .to_bytes(&mut encoded_1)
                    .expect("Encoding failed after a successful parse");

                let mut encoded_1_clone = encoded_1.clone();
                let reparsed = <$msg_type>::from_bytes(&mut encoded_1_clone)
                    .expect("Roundtrip failed: serializer produced invalid bytes");

                let mut encoded_2 = vec![0u8; reparsed.get_size()];
                reparsed
                    .clone()
                    .to_bytes(&mut encoded_2)
                    .expect("Second encoding failed");

                assert_eq!(encoded_1, encoded_2, "Serialization is not stable");
                assert!(!encoded_1.is_empty(), "Encoded output must not be empty");
                assert_eq!(
                    encoded_1.len(),
                    parsed.get_size(),
                    "Encoded length must match get_size()"
                );
                assert_eq!(
                    reparsed.get_size(),
                    parsed.get_size(),
                    "Roundtrip must preserve get_size()"
                );
                let display = parsed.to_string();
                assert_eq!(
                    display,
                    reparsed.to_string(),
                    "Display output mismatch"
                );

                let mut with_trailing = encoded_1.clone();
                with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
                let with_trailing_parsed = <$msg_type>::from_bytes(&mut with_trailing)
                    .expect(concat!(stringify!($msg_type),
                        ": trailing bytes after complete message must be ignored"));
                assert_eq!(
                    with_trailing_parsed.to_string(),
                    display,
                    "{}: trailing bytes changed decoded message",
                    stringify!($msg_type)
                );
                assert_eq!(
                    with_trailing_parsed.get_size(),
                    parsed.get_size(),
                    "{}: trailing bytes changed decoded size",
                    stringify!($msg_type)
                );
            }
        }
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
                .expect("Bool encoding failed after a successful parse");

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
                input[..encoded.len()],
                "Serialization is not stable"
            );
        }
    }};

    // ---- generic rule ----
    ($datatype:ty, $data:expr) => {{
        let mut input = $data.clone();
        let input_bytes = input.clone();

        if let Ok(parsed) = <$datatype>::from_bytes(&mut input) {
            let mut encoded = vec![0u8; parsed.get_size()];

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
                input_bytes[..encoded.len()],
                "{}: Serialization is not stable",
                stringify!($datatype)
            );

            // Spec 3.4.3: TLV fields MUST be placed at the end of the message payload.
            // Spec 3.1: data types are self-delimiting. Appending trailing bytes after
            // a complete value must not change the decoded value or size.
            let mut with_trailing = encoded.clone();
            with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
            let with_trailing_parsed = <$datatype>::from_bytes(&mut with_trailing)
                .expect(concat!(stringify!($datatype),
                    ": trailing bytes after complete value must be ignored"));
            assert_eq!(
                with_trailing_parsed,
                parsed,
                "{}: trailing bytes changed decoded value",
                stringify!($datatype)
            );

            // Spec 3.1: data types are self-delimiting. If we truncate a canonical
            // encoding and feed only a strict prefix to from_bytes, it must fail —
            // there aren't enough bytes to decode the full type.
            for cut in $crate::common::prefix_cuts(encoded.len()) {
                let mut truncated = encoded[..cut].to_vec();
                assert!(
                    <$datatype>::from_bytes(&mut truncated).is_err(),
                    "{}: strict prefix ({} of {} bytes) decoded successfully",
                    stringify!($datatype),
                    cut,
                    encoded.len()
                );
            }
        }
    }};

    // ---- generator mode: generic ----
    // Generator produces valid wire bytes. Parse must succeed.
    // Byte stability assertion.
    ($datatype:ty, $data:expr, $gen:expr) => {{
        let mut u = arbitrary::Unstructured::new(&$data);
        if let Ok(bytes) = $gen(&mut u) {
            let mut bytes = bytes;
            let parsed = <$datatype>::from_bytes(&mut bytes)
                .expect("generator produced unparseable bytes");


            let mut encoded_1 = vec![0u8; parsed.get_size()];
            parsed
                .clone()
                .to_bytes(&mut encoded_1)
                .expect("Encoding failed after a successful parse");


            let mut encoded_1_clone = encoded_1.clone();
            let reparsed = <$datatype>::from_bytes(&mut encoded_1_clone)
                .expect("Roundtrip failed: serializer produced invalid bytes");


            let mut encoded_2 = vec![0u8; reparsed.get_size()];
            reparsed
                .clone()
                .to_bytes(&mut encoded_2)
                .expect("Second encoding failed");


            assert_eq!(encoded_1, encoded_2, "Serialization is not stable");
            assert_eq!(
                encoded_1.len(),
                parsed.get_size(),
                "Encoded length must match get_size()"
            );
            assert_eq!(
                reparsed.get_size(),
                parsed.get_size(),
                "Roundtrip must preserve get_size()"
            );

            // Spec 3.4.3: TLV fields MUST be placed at the end of the message payload.
            // Spec 3.1: data types are self-delimiting. Appending trailing bytes after
            // a complete value must not change the decoded value or size.
            let mut with_trailing = encoded_1.clone();
            with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
            let with_trailing_parsed = <$datatype>::from_bytes(&mut with_trailing)
                .expect(concat!(stringify!($datatype),
                    ": trailing bytes after complete value must be ignored"));
            assert_eq!(
                with_trailing_parsed,
                parsed,
                "{}: trailing bytes changed decoded value",
                stringify!($datatype)
            );

            // Spec 3.1: data types are self-delimiting. If we truncate a canonical
            // encoding and feed only a strict prefix to from_bytes, it must fail —
            // there aren't enough bytes to decode the full type.
            for cut in $crate::common::prefix_cuts(encoded_1.len()) {
                let mut truncated = encoded_1[..cut].to_vec();
                assert!(
                    <$datatype>::from_bytes(&mut truncated).is_err(),
                    "{}: strict prefix ({} of {} bytes) decoded successfully",
                    stringify!($datatype),
                    cut,
                    encoded_1.len()
                );
            }
        }
    }};

    // ---- generator mode with spec validation: generic ----
    ($datatype:ty, $data:expr, $gen:expr, $validate:expr) => {{
        let mut u = arbitrary::Unstructured::new(&$data);
        if let Ok(bytes) = $gen(&mut u) {
            let mut bytes = bytes;
            let parsed = <$datatype>::from_bytes(&mut bytes)
                .expect("generator produced unparseable bytes");

            $validate(&parsed);

            let mut encoded_1 = vec![0u8; parsed.get_size()];
            parsed
                .clone()
                .to_bytes(&mut encoded_1)
                .expect("Encoding failed after a successful parse");

            let mut encoded_1_clone = encoded_1.clone();
            let reparsed = <$datatype>::from_bytes(&mut encoded_1_clone)
                .expect("Roundtrip failed: serializer produced invalid bytes");

            let mut encoded_2 = vec![0u8; reparsed.get_size()];
            reparsed
                .clone()
                .to_bytes(&mut encoded_2)
                .expect("Second encoding failed");

            assert_eq!(encoded_1, encoded_2, "Serialization is not stable");
            assert_eq!(
                encoded_1.len(),
                parsed.get_size(),
                "Encoded length must match get_size()"
            );
            assert_eq!(
                reparsed.get_size(),
                parsed.get_size(),
                "Roundtrip must preserve get_size()"
            );

            let mut with_trailing = encoded_1.clone();
            with_trailing.extend_from_slice(&$crate::common::TRAILING_JUNK);
            let with_trailing_parsed = <$datatype>::from_bytes(&mut with_trailing)
                .expect(concat!(stringify!($datatype),
                    ": trailing bytes after complete value must be ignored"));
            assert_eq!(
                with_trailing_parsed,
                parsed,
                "{}: trailing bytes changed decoded value",
                stringify!($datatype)
            );

            for cut in $crate::common::prefix_cuts(encoded_1.len()) {
                let mut truncated = encoded_1[..cut].to_vec();
                assert!(
                    <$datatype>::from_bytes(&mut truncated).is_err(),
                    "{}: strict prefix ({} of {} bytes) decoded successfully",
                    stringify!($datatype),
                    cut,
                    encoded_1.len()
                );
            }
        }
    }};
}

/// Bytes appended after a complete encoding to check prefix determinism.
///
/// Per spec 3.4.3: "TLV fields MUST be placed at the end of the message payload."
/// A conformant decoder must stop after the base message fields and ignore trailing bytes.
#[allow(dead_code)]
pub const TRAILING_JUNK: [u8; 6] = [0xAB, 0xCD, 0x00, 0xFF, 0x7F, 0x01];

/// Sorted, deduplicated strict prefix cut points for a given length.
///
/// Tests empty (0), midpoint (len/2), and one-byte-short (len-1) positions.
/// Used by the strict prefix rejection assertion (spec 3.1: self-delimiting types).
#[allow(dead_code)]
pub fn prefix_cuts(len: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let mut cuts = vec![len - 1, len / 2, 0];
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// WARNING: Generated with OpenAI's GPT-5.5 free model
///
/// Generate an arbitrary [`Value`] with bounded recursion depth.
///
/// Used by the SV1 fuzz targets (`fuzz_sv1_wire`, `fuzz_sv1_method_parsers`)
/// to construct random JSON inputs that exercise `serde_json::from_value`
/// and the `TryFrom` parsers.
#[allow(dead_code)]
pub fn gen_json_value(u: &mut Unstructured<'_>, depth: u8) -> arbitrary::Result<Value> {
    if depth == 0 {
        return Ok(Value::Null);
    }
    Ok(match u.int_in_range(0..=7)? {
        0 => Value::Null,
        1 => Value::Bool(u.arbitrary()?),
        2 => {
            let n: i64 = u.arbitrary()?;
            Value::Number(serde_json::Number::from(n))
        }
        3 => {
            let n: f64 = u.arbitrary()?;
            serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        4 => Value::String(u.arbitrary()?),
        5 => {
            let len = u.int_in_range(0..=3)?;
            let mut arr = Vec::with_capacity(len);
            for _ in 0..len {
                arr.push(gen_json_value(u, depth.saturating_sub(1))?);
            }
            Value::Array(arr)
        }
        6 | 7 | _ => {
            let len = u.int_in_range(0..=3)?;
            let mut map = serde_json::Map::new();
            for _ in 0..len {
                let key: String = u.arbitrary()?;
                let val = gen_json_value(u, depth.saturating_sub(1))?;
                map.insert(key, val);
            }
            Value::Object(map)
        }
    })
}
