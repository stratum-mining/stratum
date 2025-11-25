// NOTE: This is a simplified example that demonstrates UserIdentity creation.
// For full TLV encoding/decoding examples with parsers_sv2, see the parsers_sv2 crate tests
// or run integration tests from the workspace root.
//
// To run: cargo run --example worker_specific_hashrate_tracking --package extensions_sv2

use extensions_sv2::{
    UserIdentity, EXTENSION_TYPE_WORKER_HASHRATE_TRACKING, TLV_FIELD_TYPE_USER_IDENTITY,
};

fn main() {
    println!("=== Worker-Specific Hashrate Tracking Extension (Simplified) ===\n");

    // Step 1: Extension Overview
    println!("1. Extension Overview:");
    println!(
        "   Extension Type: 0x{:04x}",
        EXTENSION_TYPE_WORKER_HASHRATE_TRACKING
    );
    println!("   Purpose: Track individual worker hashrates within aggregated channels");
    println!(
        "   TLV Field Type for user_identity: 0x{:02x}\n",
        TLV_FIELD_TYPE_USER_IDENTITY
    );

    // Step 2: Creating UserIdentity instances
    println!("2. Creating UserIdentity Instances:\n");

    let workers = vec![
        "Worker_001",
        "Miner_A",
        "Device_12345678901234567890123", // 27 bytes (under 32 byte limit)
    ];

    for worker_name in workers {
        match UserIdentity::new(worker_name) {
            Ok(user_id) => {
                println!("   Created: {}", user_id);
                println!("     - Name: {}", user_id.as_str().unwrap());
                println!("     - Length: {} bytes", user_id.len());
                println!("     - Raw bytes: {:?}\n", user_id.as_bytes());
            }
            Err(e) => {
                println!("   Error creating worker '{}': {}\n", worker_name, e);
            }
        }
    }

    // Step 3: Demonstrate length limits
    println!("3. Length Validation:\n");

    // Valid: exactly 32 bytes
    let max_length_name = "x".repeat(32);
    match UserIdentity::new(&max_length_name) {
        Ok(user_id) => {
            println!("   ✓ 32-byte worker name accepted (max length)");
            println!("     Length: {} bytes\n", user_id.len());
        }
        Err(e) => {
            println!("   ✗ Unexpected error: {}\n", e);
        }
    }

    // Invalid: exceeds 32 bytes
    let too_long_name = "x".repeat(33);
    match UserIdentity::new(&too_long_name) {
        Ok(_) => {
            println!("   ✗ 33-byte worker name should have been rejected!\n");
        }
        Err(e) => {
            println!("   ✓ 33-byte worker name rejected as expected");
            println!("     Error: {}\n", e);
        }
    }

    // Step 4: Empty worker identity
    println!("4. Edge Cases:\n");

    let empty_identity = UserIdentity::new("").unwrap();
    println!("   Empty identity:");
    println!("     - Is empty: {}", empty_identity.is_empty());
    println!("     - Length: {} bytes\n", empty_identity.len());

    // Step 5: Byte-level construction
    println!("5. Creating from Raw Bytes:\n");

    let raw_bytes = b"CustomWorker";
    let identity = UserIdentity::from_bytes(raw_bytes).unwrap();
    println!("   From bytes: {:?}", raw_bytes);
    println!("   Result: {}", identity);
    println!("   As string: {}\n", identity.as_str().unwrap());

    // Step 6: Display method
    println!("6. Display Format:\n");

    let identity = UserIdentity::new("ExampleWorker").unwrap();
    println!("   Display: {}", identity);
    println!("   Debug: {:?}\n", identity);

    // Step 7: Invalid UTF-8 handling
    println!("7. Invalid UTF-8 Handling:\n");

    let invalid_utf8_bytes = vec![0xFF, 0xFE, 0xFD];
    let invalid_identity = UserIdentity::from_bytes(&invalid_utf8_bytes).unwrap();
    println!("   Bytes: {:?}", invalid_utf8_bytes);
    println!("   as_str(): {:?}", invalid_identity.as_str());
    println!(
        "   as_string_or_hex(): {}\n",
        invalid_identity.as_string_or_hex()
    );

    // Step 8: Usage context
    println!("8. Usage in Stratum V2:\n");
    println!("   When the Worker-Specific Hashrate Tracking extension (0x0002) is negotiated,");
    println!("   UserIdentity can be included as a TLV field in SubmitSharesExtended messages.");
    println!("   This allows pools to track per-worker hashrate within aggregated channels.\n");
    println!("   For full TLV encoding/decoding examples, use the parsers_sv2 crate which");
    println!("   implements the TlvField trait for UserIdentity.\n");

    println!("=== Example Complete ===");
}
