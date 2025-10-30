use binary_sv2::Seq064K;
use extensions_sv2::{
    RequestExtensions, RequestExtensionsSuccess, UserIdentity,
    EXTENSION_TYPE_WORKER_HASHRATE_TRACKING, TLV_FIELD_TYPE_USER_IDENTITY,
};
use mining_sv2::SubmitSharesExtended;
use parsers_sv2::{TlvField, TlvList, TLV_HEADER_SIZE};

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
        let tlv_bytes = TlvField::to_bytes(&user_id).unwrap();
        println!("   TLV Size: {} bytes", tlv_bytes.len());
        println!(
            "   TLV Format: [Type: 0x{:04x}|0x{:02x}] [Length: 0x{:04x}] [Value: \"{}\"]",
            EXTENSION_TYPE_WORKER_HASHRATE_TRACKING,
            TLV_FIELD_TYPE_USER_IDENTITY,
            worker_name.len(),
            worker_name
        );

        println!(
            "   Total TLV overhead: {} bytes + {} bytes (worker name) = {} bytes\n",
            TLV_HEADER_SIZE,
            worker_name.len(),
            tlv_bytes.len()
        );
    }

    // Step 5: Build a complete SubmitSharesExtended frame with TLV
    println!("5. Building a SubmitSharesExtended Frame with Worker Identity:\n");

    let submit_shares = SubmitSharesExtended {
        channel_id: 1,
        sequence_number: 42,
        job_id: 1,
        nonce: 0x12345678,
        ntime: 0x87654321,
        version: 0x20000000,
        extranonce: vec![0x01, 0x02, 0x03, 0x04].try_into().unwrap(),
    };

    // Without worker identity (empty TLV list)
    let tlv_list_empty = TlvList::from_slice(&[]).unwrap();
    let frame_bytes_without = tlv_list_empty
        .build_frame_bytes_with_tlvs(parsers_sv2::Mining::SubmitSharesExtended(
            submit_shares.clone(),
        ))
        .unwrap();
    println!(
        "   Frame bytes without worker identity: {} bytes",
        frame_bytes_without.len()
    );

    // With worker identity (using UserIdentity::to_tlv())
    let user_identity = UserIdentity::new("Worker_001").unwrap();
    let tlv = user_identity.to_tlv().unwrap();
    let tlv_list_with = TlvList::from_slice(&[tlv]).unwrap();
    let frame_bytes_with = tlv_list_with
        .build_frame_bytes_with_tlvs(parsers_sv2::Mining::SubmitSharesExtended(submit_shares))
        .unwrap();
    println!(
        "   Frame bytes with worker identity: {} bytes",
        frame_bytes_with.len()
    );
    println!(
        "   Difference: {} bytes (TLV overhead + worker name)\n",
        frame_bytes_with.len() - frame_bytes_without.len()
    );

    // Step 6: Parsing TLV fields on the receiving side (Pool)
    println!("6. Parsing TLV Fields from Received Messages:\n");
    println!("   When a pool receives SubmitSharesExtended with TLV data:");

    // Simulate TLV data that would be appended to a message
    let worker1_tlv = TlvField::to_bytes(&UserIdentity::new("Worker_001").unwrap()).unwrap();
    let worker2_tlv = TlvField::to_bytes(&UserIdentity::new("Miner_A").unwrap()).unwrap();

    // Demo 1: Parse single TLV
    println!("\n   a) Parse single worker identity:");
    let tlv_list = TlvList::from_bytes(&worker1_tlv);
    let negotiated = vec![EXTENSION_TYPE_WORKER_HASHRATE_TRACKING];
    let tlvs = tlv_list.for_extensions(&negotiated);

    // Find user_identity TLV
    for tlv in &tlvs {
        if tlv.r#type.extension_type == EXTENSION_TYPE_WORKER_HASHRATE_TRACKING
            && tlv.r#type.field_type == TLV_FIELD_TYPE_USER_IDENTITY
        {
            if let Ok(user_identity) = UserIdentity::from_tlv(tlv) {
                println!("      Found worker: {}", user_identity);
            }
        }
    }

    // Demo 2: Parse with TlvList iterator API
    println!("\n   b) Using iterator API to process TLVs:");
    for result in tlv_list.iter() {
        match result {
            Ok(tlv) => {
                println!(
                    "      - Extension: 0x{:04x}, Field: 0x{:02x}, Length: {} bytes",
                    tlv.r#type.extension_type, tlv.r#type.field_type, tlv.length
                );
                if let Ok(s) = std::str::from_utf8(&tlv.value) {
                    println!("        Value: \"{}\"", s);
                }
            }
            Err(e) => println!("      - Error parsing TLV: {:?}", e),
        }
    }

    // Demo 3: Multiple TLVs (simulating batch processing)
    println!("\n   c) Processing multiple shares with different workers:");
    let shares_with_workers = vec![
        ("Worker_001", worker1_tlv.clone()),
        ("Miner_A", worker2_tlv.clone()),
    ];

    for (expected, tlv_data) in shares_with_workers {
        let tlvs = TlvList::from_bytes(&tlv_data).for_extensions(&negotiated);
        for tlv in &tlvs {
            if tlv.r#type.extension_type == EXTENSION_TYPE_WORKER_HASHRATE_TRACKING
                && tlv.r#type.field_type == TLV_FIELD_TYPE_USER_IDENTITY
            {
                if let Ok(worker) = UserIdentity::from_tlv(tlv) {
                    println!("      Share from: {} (expected: {})", worker, expected);
                }
            }
        }
    }

    // Demo 4: Using find() to search for specific TLV
    println!("\n   d) Find specific TLV field:");
    if let Some(tlv) = TlvList::from_bytes(&worker1_tlv).find(
        EXTENSION_TYPE_WORKER_HASHRATE_TRACKING,
        TLV_FIELD_TYPE_USER_IDENTITY,
    ) {
        println!(
            "      Found user_identity TLV with {} bytes",
            tlv.value.len()
        );
        if let Ok(s) = std::str::from_utf8(&tlv.value) {
            println!("      Worker: \"{}\"", s);
        }
    }

    println!();

    // Step 7: Convenience methods for applications
    println!("7. Simplified API for Applications:\n");

    println!("   a) Convert UserIdentity to TLV:");
    let user_identity = UserIdentity::new("Worker_001").unwrap();
    let tlv = TlvField::to_tlv(&user_identity).unwrap();
    println!(
        "      Created TLV: extension_type=0x{:04x}, field_type=0x{:02x}",
        tlv.r#type.extension_type, tlv.r#type.field_type
    );

    println!("\n   b) Create Vec<Tlv> from UserIdentity:");
    let tlv_vec = vec![TlvField::to_tlv(&user_identity).unwrap()];
    println!("      Created Vec<Tlv> with {} element(s)", tlv_vec.len());
    println!(
        "      Usage: channel_sender.send((Mining::SubmitSharesExtended(msg), Some(tlv_vec)))?;"
    );

    println!("\n   This simplifies the application code:");
    println!("      let user_identity = UserIdentity::new(worker_name)?;");
    println!("      let tlv = TlvField::to_tlv(&user_identity)?;");
    println!("      let tlv_vec = vec![tlv];");

    println!();

    // Step 9: Use case example
    println!("9. Use Case - Mining Farm with Multiple Workers:");
    println!("   A mining farm with 100 devices can use a single extended channel");
    println!("   and track per-device hashrate by including the device worker_name in each share.");
    println!("   This allows the pool to monitor individual device performance while");
    println!("   reducing connection overhead compared to 100 separate channels.\n");

    println!("=== Example Complete ===");
}
