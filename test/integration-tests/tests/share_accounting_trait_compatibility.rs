//! Integration tests for ShareAccountingTrait behavioral compatibility.
//!
//! These tests verify that the new trait-based ShareAccounting implementation
//! produces identical behavior to the existing client and server implementations.
//! This ensures backward compatibility and validates the trait abstraction.

use bitcoin::hashes::sha256d::Hash;
use bitcoin::hashes::Hash as HashTrait;
use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
use channels_sv2::share_accounting_trait::{
    create_client_share_accounting, create_server_share_accounting, ShareAccountingConfig,
    create_share_accounting,
};

/// Test data structure for share validation scenarios
#[derive(Clone)]
struct ShareTestData {
    work: u64,
    sequence_number: u32,
    hash: Hash,
    difficulty: f64,
}

impl ShareTestData {
    fn new(work: u64, sequence_number: u32, hash_bytes: [u8; 32], difficulty: f64) -> Self {
        Self {
            work,
            sequence_number,
            hash: Hash::from_slice(&hash_bytes).unwrap(),
            difficulty,
        }
    }
}

/// Generate test shares with varying work and difficulties
fn generate_test_shares() -> Vec<ShareTestData> {
    vec![
        ShareTestData::new(1000, 1, [1u8; 32], 1000.0),
        ShareTestData::new(2000, 2, [2u8; 32], 1500.0),
        ShareTestData::new(1500, 3, [3u8; 32], 800.0),
        ShareTestData::new(3000, 4, [4u8; 32], 2500.0),
        ShareTestData::new(500, 5, [5u8; 32], 600.0),
        ShareTestData::new(2500, 6, [6u8; 32], 3000.0),
        ShareTestData::new(1800, 7, [7u8; 32], 1200.0),
        ShareTestData::new(4000, 8, [8u8; 32], 4500.0),
        ShareTestData::new(1200, 9, [9u8; 32], 900.0),
        ShareTestData::new(2200, 10, [10u8; 32], 2000.0),
    ]
}

#[test]
fn test_client_mode_behavioral_compatibility() {
    // Create both implementations
    let mut original_client = ClientShareAccounting::new();
    let mut trait_client = create_client_share_accounting();
    
    let test_shares = generate_test_shares();
    
    // Verify initial state compatibility
    assert_eq!(
        original_client.get_shares_accepted(),
        trait_client.get_shares_accepted().unwrap()
    );
    assert_eq!(
        original_client.get_share_work_sum(),
        trait_client.get_share_work_sum().unwrap()
    );
    assert_eq!(
        original_client.get_last_share_sequence_number(),
        trait_client.get_last_share_sequence_number().unwrap()
    );
    assert_eq!(
        original_client.get_best_diff(),
        trait_client.get_best_diff().unwrap()
    );
    
    // Process shares and verify identical behavior
    for share in &test_shares {
        // Update both implementations
        original_client.update_share_accounting(
            share.work,
            share.sequence_number,
            share.hash,
        );
        trait_client.update_share_accounting(
            share.work,
            share.sequence_number,
            share.hash,
        ).unwrap();
        
        // Update difficulty in both
        original_client.update_best_diff(share.difficulty);
        trait_client.update_best_diff(share.difficulty).unwrap();
        
        // Verify identical state after each share
        assert_eq!(
            original_client.get_shares_accepted(),
            trait_client.get_shares_accepted().unwrap(),
            "Share count mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_client.get_share_work_sum(),
            trait_client.get_share_work_sum().unwrap(),
            "Work sum mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_client.get_last_share_sequence_number(),
            trait_client.get_last_share_sequence_number().unwrap(),
            "Sequence number mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_client.get_best_diff(),
            trait_client.get_best_diff().unwrap(),
            "Best difficulty mismatch after processing share {}", share.sequence_number
        );
        
        // Verify duplicate detection behavior
        assert_eq!(
            original_client.is_share_seen(share.hash),
            trait_client.is_share_seen(share.hash).unwrap(),
            "Duplicate detection mismatch for share {}", share.sequence_number
        );
    }
    
    // Test flush behavior
    original_client.flush_seen_shares();
    trait_client.flush_seen_shares().unwrap();
    
    // Verify shares are no longer seen after flush
    for share in &test_shares {
        assert_eq!(
            original_client.is_share_seen(share.hash),
            trait_client.is_share_seen(share.hash).unwrap(),
            "Post-flush duplicate detection mismatch for share {}", share.sequence_number
        );
    }
    
    // Verify statistics remain unchanged after flush
    assert_eq!(
        original_client.get_shares_accepted(),
        trait_client.get_shares_accepted().unwrap()
    );
    assert_eq!(
        original_client.get_share_work_sum(),
        trait_client.get_share_work_sum().unwrap()
    );
    assert_eq!(
        original_client.get_best_diff(),
        trait_client.get_best_diff().unwrap()
    );
}

#[test]
fn test_server_mode_behavioral_compatibility() {
    let batch_size = 3;
    
    // Create both implementations
    let mut original_server = ServerShareAccounting::new(batch_size);
    let mut trait_server = create_server_share_accounting(batch_size);
    
    let test_shares = generate_test_shares();
    
    // Verify initial state compatibility
    assert_eq!(
        original_server.get_shares_accepted(),
        trait_server.get_shares_accepted().unwrap()
    );
    assert_eq!(
        original_server.get_share_work_sum(),
        trait_server.get_share_work_sum().unwrap()
    );
    assert_eq!(
        original_server.get_last_share_sequence_number(),
        trait_server.get_last_share_sequence_number().unwrap()
    );
    assert_eq!(
        original_server.get_best_diff(),
        trait_server.get_best_diff().unwrap()
    );
    assert_eq!(
        original_server.get_share_batch_size(),
        trait_server.get_share_batch_size().unwrap().unwrap()
    );
    // Note: Original server implementation returns true for should_acknowledge() when shares_accepted == 0
    // because 0 % batch_size == 0. This is maintained for compatibility.
    assert_eq!(
        original_server.should_acknowledge(),
        trait_server.should_acknowledge().unwrap()
    );
    
    // Process shares and verify identical behavior including batch acknowledgments
    for (i, share) in test_shares.iter().enumerate() {
        // Update both implementations
        original_server.update_share_accounting(
            share.work,
            share.sequence_number,
            share.hash,
        );
        trait_server.update_share_accounting(
            share.work,
            share.sequence_number,
            share.hash,
        ).unwrap();
        
        // Update difficulty in both
        original_server.update_best_diff(share.difficulty);
        trait_server.update_best_diff(share.difficulty).unwrap();
        
        // Verify identical state after each share
        assert_eq!(
            original_server.get_shares_accepted(),
            trait_server.get_shares_accepted().unwrap(),
            "Share count mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_server.get_share_work_sum(),
            trait_server.get_share_work_sum().unwrap(),
            "Work sum mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_server.get_last_share_sequence_number(),
            trait_server.get_last_share_sequence_number().unwrap(),
            "Sequence number mismatch after processing share {}", share.sequence_number
        );
        assert_eq!(
            original_server.get_best_diff(),
            trait_server.get_best_diff().unwrap(),
            "Best difficulty mismatch after processing share {}", share.sequence_number
        );
        
        // Verify batch acknowledgment behavior
        assert_eq!(
            original_server.should_acknowledge(),
            trait_server.should_acknowledge().unwrap(),
            "Batch acknowledgment mismatch after processing share {} (share count: {})", 
            share.sequence_number, i + 1
        );
        
        // Verify duplicate detection behavior
        assert_eq!(
            original_server.is_share_seen(share.hash),
            trait_server.is_share_seen(share.hash).unwrap(),
            "Duplicate detection mismatch for share {}", share.sequence_number
        );
    }
}

#[test]
fn test_batch_acknowledgment_logic_compatibility() {
    let test_cases = vec![1, 2, 3, 5, 10];
    
    for batch_size in test_cases {
        let mut original_server = ServerShareAccounting::new(batch_size);
        let mut trait_server = create_server_share_accounting(batch_size);
        
        // Verify initial acknowledgment behavior (both should return true when shares_accepted == 0)
        assert_eq!(
            original_server.should_acknowledge(),
            trait_server.should_acknowledge().unwrap(),
            "Initial acknowledgment mismatch for batch_size={}", batch_size
        );
        
        // Test acknowledgment behavior for multiple batches
        for share_num in 1..=(batch_size * 3) {
            let share_hash = Hash::from_slice(&[share_num as u8; 32]).unwrap();
            
            // Process share in both implementations
            original_server.update_share_accounting(1000, share_num as u32, share_hash);
            trait_server.update_share_accounting(1000, share_num as u32, share_hash).unwrap();
            
            // Verify acknowledgment behavior matches exactly
            assert_eq!(
                original_server.should_acknowledge(),
                trait_server.should_acknowledge().unwrap(),
                "Batch acknowledgment mismatch for batch_size={}, share_num={}", 
                batch_size, share_num
            );
        }
    }
}

#[test]
fn test_duplicate_detection_workflow_compatibility() {
    let mut original_client = ClientShareAccounting::new();
    let mut trait_client = create_client_share_accounting();
    
    let test_shares = generate_test_shares();
    
    // Process all shares
    for share in &test_shares {
        original_client.update_share_accounting(share.work, share.sequence_number, share.hash);
        trait_client.update_share_accounting(share.work, share.sequence_number, share.hash).unwrap();
    }
    
    // Verify all shares are seen
    for share in &test_shares {
        assert_eq!(
            original_client.is_share_seen(share.hash),
            trait_client.is_share_seen(share.hash).unwrap()
        );
        assert!(trait_client.is_share_seen(share.hash).unwrap());
    }
    
    // Test duplicate submission detection
    let duplicate_share = &test_shares[0];
    assert!(original_client.is_share_seen(duplicate_share.hash));
    assert!(trait_client.is_share_seen(duplicate_share.hash).unwrap());
    
    // Flush and verify shares are no longer seen
    original_client.flush_seen_shares();
    trait_client.flush_seen_shares().unwrap();
    
    for share in &test_shares {
        assert_eq!(
            original_client.is_share_seen(share.hash),
            trait_client.is_share_seen(share.hash).unwrap()
        );
        assert!(!trait_client.is_share_seen(share.hash).unwrap());
    }
    
    // Re-process one share and verify it's detected again
    let reprocessed_share = &test_shares[0];
    original_client.update_share_accounting(
        reprocessed_share.work,
        reprocessed_share.sequence_number + 100, // Different sequence number
        reprocessed_share.hash,
    );
    trait_client.update_share_accounting(
        reprocessed_share.work,
        reprocessed_share.sequence_number + 100,
        reprocessed_share.hash,
    ).unwrap();
    
    assert!(original_client.is_share_seen(reprocessed_share.hash));
    assert!(trait_client.is_share_seen(reprocessed_share.hash).unwrap());
}

#[test]
fn test_per_user_difficulty_tracking() {
    let mut trait_accounting = create_client_share_accounting();
    
    // Test per-user difficulty tracking (new functionality)
    trait_accounting.update_best_diff_for_user("alice", 1000.0).unwrap();
    trait_accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
    trait_accounting.update_best_diff_for_user("charlie", 1500.0).unwrap();
    
    // Verify per-user difficulties
    assert_eq!(
        trait_accounting.get_best_diff_for_user("alice").unwrap(),
        Some(1000.0)
    );
    assert_eq!(
        trait_accounting.get_best_diff_for_user("bob").unwrap(),
        Some(2000.0)
    );
    assert_eq!(
        trait_accounting.get_best_diff_for_user("charlie").unwrap(),
        Some(1500.0)
    );
    assert_eq!(
        trait_accounting.get_best_diff_for_user("nonexistent").unwrap(),
        None
    );
    
    // Verify overall best difficulty is the maximum
    assert_eq!(trait_accounting.get_best_diff().unwrap(), 2000.0);
    
    // Test difficulty updates (should only update if higher)
    trait_accounting.update_best_diff_for_user("alice", 500.0).unwrap(); // Lower, should not update
    trait_accounting.update_best_diff_for_user("alice", 2500.0).unwrap(); // Higher, should update
    
    assert_eq!(
        trait_accounting.get_best_diff_for_user("alice").unwrap(),
        Some(2500.0)
    );
    assert_eq!(trait_accounting.get_best_diff().unwrap(), 2500.0);
    
    // Test top users query
    let top_users = trait_accounting.get_top_user_difficulties(2).unwrap();
    assert_eq!(top_users.len(), 2);
    assert_eq!(top_users[0], ("alice".to_string(), 2500.0));
    assert_eq!(top_users[1], ("bob".to_string(), 2000.0));
    
    // Test backward compatibility with update_best_diff
    trait_accounting.update_best_diff(3000.0).unwrap();
    assert_eq!(trait_accounting.get_best_diff().unwrap(), 3000.0);
}

#[test]
fn test_configuration_and_factory_functions() {
    // Test client configuration
    let client_config = ShareAccountingConfig::InMemory { share_batch_size: None };
    let client_accounting = create_share_accounting(client_config).unwrap();
    assert_eq!(client_accounting.get_share_batch_size().unwrap(), None);
    assert!(!client_accounting.should_acknowledge().unwrap());
    
    // Test server configuration
    let server_config = ShareAccountingConfig::InMemory { share_batch_size: Some(5) };
    let server_accounting = create_share_accounting(server_config).unwrap();
    assert_eq!(server_accounting.get_share_batch_size().unwrap(), Some(5));
    assert!(server_accounting.should_acknowledge().unwrap()); // Initially true (0 % batch_size == 0)
    
    // Test invalid configuration
    let invalid_config = ShareAccountingConfig::InMemory { share_batch_size: Some(0) };
    assert!(create_share_accounting(invalid_config).is_err());
}

#[test]
fn test_end_to_end_share_validation_workflow() {
    let batch_size = 3;
    let mut server_accounting = create_server_share_accounting(batch_size);
    
    let test_shares = generate_test_shares();
    
    // Simulate complete share validation workflow
    for (i, share) in test_shares.iter().enumerate() {
        // 1. Check for duplicate before processing
        assert!(!server_accounting.is_share_seen(share.hash).unwrap());
        
        // 2. Process the share
        server_accounting.update_share_accounting(
            share.work,
            share.sequence_number,
            share.hash,
        ).unwrap();
        
        // 3. Update difficulty tracking
        server_accounting.update_best_diff_for_user(
            &format!("user_{}", i % 3), // Simulate multiple users
            share.difficulty,
        ).unwrap();
        
        // 4. Check if acknowledgment is needed
        let should_ack = server_accounting.should_acknowledge().unwrap();
        let expected_ack = (i + 1) % batch_size == 0;
        assert_eq!(should_ack, expected_ack);
        
        // 5. Verify share is now seen
        assert!(server_accounting.is_share_seen(share.hash).unwrap());
        
        // 6. Verify statistics are updated correctly
        assert_eq!(server_accounting.get_shares_accepted().unwrap(), (i + 1) as u32);
        assert_eq!(
            server_accounting.get_share_work_sum().unwrap(),
            test_shares[..=i].iter().map(|s| s.work).sum::<u64>()
        );
        assert_eq!(
            server_accounting.get_last_share_sequence_number().unwrap(),
            share.sequence_number
        );
    }
    
    // Test chain tip update simulation (flush seen shares)
    server_accounting.flush_seen_shares().unwrap();
    
    // Verify shares are no longer seen but statistics remain
    for share in &test_shares {
        assert!(!server_accounting.is_share_seen(share.hash).unwrap());
    }
    assert_eq!(server_accounting.get_shares_accepted().unwrap(), test_shares.len() as u32);
    assert_eq!(
        server_accounting.get_share_work_sum().unwrap(),
        test_shares.iter().map(|s| s.work).sum::<u64>()
    );
}

#[test]
fn test_client_vs_server_mode_differences() {
    let mut client = create_client_share_accounting();
    let mut server = create_server_share_accounting(3);
    
    // Test configuration differences
    assert_eq!(client.get_share_batch_size().unwrap(), None);
    assert_eq!(server.get_share_batch_size().unwrap(), Some(3));
    
    // Process identical shares in both
    let test_shares = &generate_test_shares()[..6]; // Process 6 shares
    
    for share in test_shares {
        client.update_share_accounting(share.work, share.sequence_number, share.hash).unwrap();
        server.update_share_accounting(share.work, share.sequence_number, share.hash).unwrap();
    }
    
    // Verify identical statistics
    assert_eq!(
        client.get_shares_accepted().unwrap(),
        server.get_shares_accepted().unwrap()
    );
    assert_eq!(
        client.get_share_work_sum().unwrap(),
        server.get_share_work_sum().unwrap()
    );
    
    // Verify different acknowledgment behavior
    assert!(!client.should_acknowledge().unwrap()); // Client never acknowledges
    assert!(server.should_acknowledge().unwrap()); // Server acknowledges after 6 shares (2 batches of 3)
}