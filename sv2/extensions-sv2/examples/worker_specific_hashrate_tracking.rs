use binary_sv2::Seq064K;
use extensions_sv2::{
    build_submit_shares_extended_with_user_identity_frame, encode_user_identity_as_tlv_bytes,
    RequestExtensions, RequestExtensionsSuccess, UserIdentity,
    EXTENSION_TYPE_WORKER_HASHRATE_TRACKING, TLV_FIELD_TYPE_USER_IDENTITY, TLV_HEADER_SIZE,
};
use mining_sv2::SubmitSharesExtended;

fn main() {
    println!("=== Worker-Specific Hashrate Tracking Extension Example ===\n");

    // Step 1: Client requests the Worker Hashrate Tracking extension
    println!("1. Extension Negotiation:");
    println!("   Client requests Worker Hashrate Tracking extension (0x0002)");

    let request = RequestExtensions {
        request_id: 1,
        requested_extensions: Seq064K::new(vec![EXTENSION_TYPE_WORKER_HASHRATE_TRACKING]).unwrap(),
    };
    println!("   {}", request);
    println!(
        "   Extension type: 0x{:04x}\n",
        EXTENSION_TYPE_WORKER_HASHRATE_TRACKING
    );

    // Step 2: Server accepts the extension
    println!("2. Server Response:");
    let success = RequestExtensionsSuccess {
        request_id: 1,
        supported_extensions: Seq064K::new(vec![EXTENSION_TYPE_WORKER_HASHRATE_TRACKING]).unwrap(),
    };
    println!("   {}", success);
    println!(
        "   Extension 0x{:04x} is now active!\n",
        EXTENSION_TYPE_WORKER_HASHRATE_TRACKING
    );

    // Step 3: Demonstrate TLV field for user_identity
    println!("3. TLV Field Structure:");
    println!("   When submitting shares, the client appends a TLV field:");
    println!(
        "   - Type (U16 | U8): Extension Type (0x{:04x}) | Field Type (0x{:02x})",
        EXTENSION_TYPE_WORKER_HASHRATE_TRACKING, TLV_FIELD_TYPE_USER_IDENTITY
    );
    println!("   - Length (U16): Actual byte length of worker name");
    println!("   - Value: UTF-8 encoded worker identifier\n");

    // Step 4: Create UserIdentity TLV fields for different workers
    println!("4. Example TLV Fields for Different Workers:\n");

    let workers = vec!["Worker_001", "Miner_A", "Device_12345678901234567890123"];

    for worker_name in workers {
        let user_id = UserIdentity::new(worker_name).unwrap();

        println!("   {}", user_id);
        let tlv_bytes = encode_user_identity_as_tlv_bytes(&user_id).unwrap();
        println!("   TLV Size: {} bytes", tlv_bytes.len());
        println!(
            "   TLV Format: [Type: 0x{:04x}|0x{:02x}] [Length: 0x{:04x}] [Value: \"{}\"]",
            EXTENSION_TYPE_WORKER_HASHRATE_TRACKING,
            TLV_FIELD_TYPE_USER_IDENTITY,
            worker_name.len(),
            worker_name
        );

        // Show the binary representation conceptually
        println!(
            "   Total TLV overhead: {} bytes + {} bytes (worker name) = {} bytes\n",
            TLV_HEADER_SIZE,
            worker_name.len(),
            tlv_bytes.len()
        );
    }

    // Step 5: Bandwidth consideration
    println!("5. Bandwidth Impact:");
    println!("   Base SubmitSharesExtended message: ~70 bytes");
    println!(
        "   With max UserIdentity (32 bytes): ~{} bytes ({} bytes TLV overhead + 32 bytes value)",
        70 + TLV_HEADER_SIZE + 32,
        TLV_HEADER_SIZE
    );
    println!(
        "   With avg UserIdentity (10 bytes): ~{} bytes ({} bytes TLV overhead + 10 bytes value)",
        70 + TLV_HEADER_SIZE + 10,
        TLV_HEADER_SIZE
    );
    println!("\n   For 10 shares/minute:");
    println!(
        "   - Max impact: {} bytes/min or ~{} KB/hour",
        10 * (TLV_HEADER_SIZE + 32),
        (10 * (TLV_HEADER_SIZE + 32) * 60) / 1024
    );
    println!(
        "   - Avg impact: {} bytes/min or ~{} KB/hour\n",
        10 * (TLV_HEADER_SIZE + 10),
        (10 * (TLV_HEADER_SIZE + 10) * 60) / 1024
    );

    // Step 6: Build a complete SubmitSharesExtended frame with TLV
    println!("6. Building a SubmitSharesExtended Frame with Worker Identity:\n");

    let submit_shares = SubmitSharesExtended {
        channel_id: 1,
        sequence_number: 42,
        job_id: 1,
        nonce: 0x12345678,
        ntime: 0x87654321,
        version: 0x20000000,
        extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
    };

    // Without worker identity
    let frame_without =
        build_submit_shares_extended_with_user_identity_frame(submit_shares.clone(), "").unwrap();
    println!(
        "   Frame without worker identity: {} bytes",
        frame_without.encoded_length()
    );

    // With worker identity
    let frame_with =
        build_submit_shares_extended_with_user_identity_frame(submit_shares, "Worker_001").unwrap();
    println!(
        "   Frame with worker identity: {} bytes",
        frame_with.encoded_length()
    );
    println!(
        "   Difference: {} bytes (TLV overhead + worker name)\n",
        frame_with.encoded_length() - frame_without.encoded_length()
    );

    // Step 7: Use case example
    println!("7. Use Case - Mining Farm with Multiple Workers:");
    println!("   A mining farm with 100 devices can use a single extended channel");
    println!("   and track per-device hashrate by including the device worker_name in each share.");
    println!("   This allows the pool to monitor individual device performance while");
    println!("   reducing connection overhead compared to 100 separate channels.\n");

    println!("=== Example Complete ===");
}
