# ShareAccountingTrait Implementation Guide

This guide provides comprehensive documentation for implementing and using the `ShareAccountingTrait`, including migration from existing concrete implementations, usage patterns, and best practices.

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Migration Guide](#migration-guide)
4. [Usage Patterns](#usage-patterns)
5. [Implementation Guidelines](#implementation-guidelines)
6. [Performance Considerations](#performance-considerations)
7. [Troubleshooting](#troubleshooting)

## Overview

The `ShareAccountingTrait` provides a storage-agnostic interface for share accounting operations in Stratum V2 implementations. It abstracts the underlying storage mechanism while maintaining compatibility with existing client and server implementations.

### Key Features

- **Storage Agnostic**: Support for different storage backends (in-memory, database, file-based)
- **Client/Server Support**: Unified interface for both mining clients and servers
- **Per-User Tracking**: Advanced difficulty tracking per user identity
- **Duplicate Detection**: Efficient share hash duplicate detection
- **Batch Acknowledgments**: Server-side batch acknowledgment logic
- **Migration Support**: Utilities for migrating from existing concrete implementations

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ShareAccountingTrait                     │
├─────────────────────────────────────────────────────────────┤
│  • update_share_accounting()                                │
│  • is_share_seen() / flush_seen_shares()                    │
│  • get_shares_accepted() / get_share_work_sum()             │
│  • get_best_diff() / update_best_diff()                     │
│  • get_best_diff_for_user() / update_best_diff_for_user()   │
│  • get_top_user_difficulties()                              │
│  • should_acknowledge() / get_share_batch_size()            │
└─────────────────────────────────────────────────────────────┘
                                │
                ┌───────────────┼───────────────┐
                │               │               │
    ┌───────────▼────────┐ ┌────▼────────┐ ┌───▼──────────┐
    │ InMemoryShare      │ │ DatabaseShare│ │ FileShare    │
    │ Accounting         │ │ Accounting   │ │ Accounting   │
    │ (Current)          │ │ (Future)     │ │ (Future)     │
    └────────────────────┘ └─────────────┘ └──────────────┘
```

## Quick Start

### Client Usage

```rust
use channels_sv2::share_accounting_trait::*;
use bitcoin::hashes::sha256d::Hash;
use bitcoin::hashes::Hash as HashTrait;

// Create a client-mode share accounting instance
let mut accounting = create_client_share_accounting();

// Process a share
let share_hash = Hash::from_slice(&[1u8; 32]).unwrap();
accounting.update_share_accounting(1000, 1, share_hash).unwrap();

// Check statistics
assert_eq!(accounting.get_shares_accepted().unwrap(), 1);
assert_eq!(accounting.get_share_work_sum().unwrap(), 1000);
assert!(!accounting.should_acknowledge().unwrap()); // Client never acknowledges
```

### Server Usage

```rust
use channels_sv2::share_accounting_trait::*;
use bitcoin::hashes::sha256d::Hash;
use bitcoin::hashes::Hash as HashTrait;

// Create a server-mode share accounting instance with batch size 10
let mut accounting = create_server_share_accounting(10);

// Process shares
for i in 1..=10 {
    let share_hash = Hash::from_slice(&[i; 32]).unwrap();
    accounting.update_share_accounting(1000, i as u32, share_hash).unwrap();
}

// Check if we should send a batch acknowledgment
if accounting.should_acknowledge().unwrap() {
    println!("Send batch acknowledgment for {} shares", 
             accounting.get_shares_accepted().unwrap());
}
```

## Migration Guide

### From Client ShareAccounting

If you're currently using the concrete `client::share_accounting::ShareAccounting`:

```rust
// Before: Concrete implementation
use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
let mut old_accounting = ClientShareAccounting::new();

// After: Trait-based implementation
use channels_sv2::share_accounting_trait::*;
let mut new_accounting = create_client_share_accounting();

// Or migrate existing instance
use channels_sv2::share_accounting_trait::migration::*;
let migrated = migrate_from_client_share_accounting(old_accounting);
```

### From Server ShareAccounting

If you're currently using the concrete `server::share_accounting::ShareAccounting`:

```rust
// Before: Concrete implementation
use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
let mut old_accounting = ServerShareAccounting::new(10);

// After: Trait-based implementation
use channels_sv2::share_accounting_trait::*;
let mut new_accounting = create_server_share_accounting(10);

// Or migrate existing instance
use channels_sv2::share_accounting_trait::migration::*;
let migrated = migrate_from_server_share_accounting(old_accounting);
```

### Migration Checklist

- [ ] Replace concrete types with trait objects
- [ ] Update function signatures to accept `&mut dyn ShareAccountingTrait`
- [ ] Add `.unwrap()` calls for `Result` return types (or proper error handling)
- [ ] Update imports to use the trait module
- [ ] Test that behavior remains identical
- [ ] Consider using per-user difficulty tracking features

## Usage Patterns

### Generic Functions

Write functions that work with any implementation:

```rust
fn process_mining_shares<T: ShareAccountingTrait>(
    accounting: &mut T,
    shares: Vec<(u64, u32, Hash)>
) -> Result<Vec<bool>, T::Error> {
    let mut acknowledgments = Vec::new();
    
    for (work, seq, hash) in shares {
        // Skip duplicates
        if accounting.is_share_seen(hash)? {
            acknowledgments.push(false);
            continue;
        }
        
        // Process the share
        accounting.update_share_accounting(work, seq, hash)?;
        acknowledgments.push(accounting.should_acknowledge()?);
    }
    
    Ok(acknowledgments)
}
```

### Per-User Difficulty Tracking

Track difficulties for individual users:

```rust
let mut accounting = create_server_share_accounting(10);

// Update difficulties for different users
accounting.update_best_diff_for_user("alice", 1500.0).unwrap();
accounting.update_best_diff_for_user("bob", 2000.0).unwrap();
accounting.update_best_diff_for_user("charlie", 1200.0).unwrap();

// Get top performers
let top_users = accounting.get_top_user_difficulties(5).unwrap();
for (user, difficulty) in top_users {
    println!("User {} has best difficulty: {}", user, difficulty);
}

// Get overall best difficulty (max across all users)
let overall_best = accounting.get_best_diff().unwrap();
println!("Overall best difficulty: {}", overall_best);
```

### Chain Tip Updates

Handle blockchain reorganizations:

```rust
fn handle_chain_tip_update<T: ShareAccountingTrait>(
    accounting: &mut T
) -> Result<(), T::Error> {
    // Clear duplicate detection cache for new chain tip
    accounting.flush_seen_shares()?;
    
    // Statistics (shares accepted, work sum, difficulties) are preserved
    println!("Chain tip updated. Shares accepted: {}", 
             accounting.get_shares_accepted()?);
    
    Ok(())
}
```

### Error Handling

Robust error handling patterns:

```rust
fn safe_share_processing(
    accounting: &mut dyn ShareAccountingTrait<Error = InMemoryShareAccountingError>,
    work: u64,
    seq: u32,
    hash: Hash
) -> Result<Option<BatchAcknowledgment>, Box<dyn std::error::Error>> {
    // Check for duplicates
    if accounting.is_share_seen(hash)? {
        return Ok(None); // Duplicate share
    }
    
    // Process the share
    accounting.update_share_accounting(work, seq, hash)?;
    
    // Check if we need to send acknowledgment
    if accounting.should_acknowledge()? {
        let batch_info = BatchAcknowledgment {
            last_sequence_number: accounting.get_last_share_sequence_number()?,
            shares_accepted: accounting.get_shares_accepted()?,
            work_sum: accounting.get_share_work_sum()?,
        };
        Ok(Some(batch_info))
    } else {
        Ok(None)
    }
}

struct BatchAcknowledgment {
    last_sequence_number: u32,
    shares_accepted: u32,
    work_sum: u64,
}
```

## Implementation Guidelines

### Custom Storage Backend

To implement a custom storage backend:

```rust
use channels_sv2::share_accounting_trait::ShareAccountingTrait;
use std::collections::{HashSet, BTreeMap};

pub struct DatabaseShareAccounting {
    // Keep duplicate detection in memory for performance
    seen_shares: HashSet<Hash>,
    
    // Database connection
    db_connection: DatabaseConnection,
    
    // Configuration
    share_batch_size: Option<usize>,
    channel_id: String,
}

#[derive(Debug)]
pub enum DatabaseError {
    ConnectionError(String),
    QueryError(String),
    SerializationError(String),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::ConnectionError(msg) => write!(f, "Database connection error: {}", msg),
            DatabaseError::QueryError(msg) => write!(f, "Database query error: {}", msg),
            DatabaseError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl ShareAccountingTrait for DatabaseShareAccounting {
    type Error = DatabaseError;
    
    fn update_share_accounting(
        &mut self,
        share_work: u64,
        share_sequence_number: u32,
        share_hash: Hash,
    ) -> Result<(), Self::Error> {
        // Update in-memory duplicate detection cache
        self.seen_shares.insert(share_hash);
        
        // Persist to database
        self.db_connection.execute(
            "UPDATE channel_stats SET 
             last_sequence_number = ?, 
             shares_accepted = shares_accepted + 1, 
             work_sum = work_sum + ? 
             WHERE channel_id = ?",
            &[
                &share_sequence_number,
                &share_work,
                &self.channel_id
            ]
        ).map_err(|e| DatabaseError::QueryError(e.to_string()))?;
        
        Ok(())
    }
    
    fn get_shares_accepted(&self) -> Result<u32, Self::Error> {
        let result = self.db_connection.query_row(
            "SELECT shares_accepted FROM channel_stats WHERE channel_id = ?",
            &[&self.channel_id],
            |row| row.get(0)
        ).map_err(|e| DatabaseError::QueryError(e.to_string()))?;
        
        Ok(result)
    }
    
    // ... implement other methods
}
```

### Configuration Extension

Extend the configuration enum for your backend:

```rust
#[derive(Debug, Clone)]
pub enum ShareAccountingConfig {
    InMemory {
        share_batch_size: Option<usize>,
    },
    Database {
        connection_string: String,
        share_batch_size: Option<usize>,
        table_prefix: String,
    },
    File {
        file_path: PathBuf,
        share_batch_size: Option<usize>,
        sync_interval: Duration,
    },
}
```

## Performance Considerations

### Benchmarking

Run the included benchmarks to compare performance:

```bash
cd protocols/v2/channels-sv2
cargo bench --bench share_accounting_benchmarks
```

The benchmarks compare:
- Share processing throughput
- Duplicate detection performance
- Batch acknowledgment logic
- Difficulty tracking operations
- Memory usage patterns

### Optimization Tips

1. **Duplicate Detection**: Keep the seen shares cache in memory regardless of storage backend
2. **Batch Operations**: For database backends, consider batching multiple share updates
3. **Caching**: Cache frequently accessed statistics in memory
4. **Indexing**: Ensure proper database indexes for user difficulty queries
5. **Connection Pooling**: Use connection pooling for database backends

### Memory Usage

The in-memory implementation has the following characteristics:
- **Share Hashes**: O(n) where n is the number of unique shares since last flush
- **User Difficulties**: O(u) where u is the number of unique users
- **Statistics**: O(1) primitive values

## Troubleshooting

### Common Issues

#### Migration Compilation Errors

```rust
// Error: method not found in `Box<dyn ShareAccountingTrait>`
let result = accounting.some_method();

// Solution: Add .unwrap() for Result types
let result = accounting.some_method().unwrap();

// Or handle errors properly
let result = accounting.some_method()
    .map_err(|e| format!("Share accounting error: {}", e))?;
```

#### Performance Regression

If you notice performance issues after migration:

1. Run benchmarks to identify bottlenecks
2. Check if you're using trait objects efficiently
3. Consider using concrete types in hot paths
4. Profile memory allocations

#### State Migration Issues

If migrated state doesn't match original:

```rust
// Verify state preservation after migration
let original = ClientShareAccounting::new();
// ... populate original with test data ...

let migrated = migrate_from_client_share_accounting(original.clone());

assert_eq!(migrated.get_shares_accepted().unwrap(), original.get_shares_accepted());
assert_eq!(migrated.get_share_work_sum().unwrap(), original.get_share_work_sum());
// Note: seen_shares cache is not migrated - this is expected
```

### Debug Tips

1. **Enable Logging**: Use the `tracing` crate for detailed logging
2. **Test Coverage**: Write comprehensive tests for your implementation
3. **Error Propagation**: Ensure errors are properly propagated and logged
4. **State Validation**: Regularly validate internal state consistency

### Getting Help

- Check the trait documentation for method specifications
- Review the included examples and tests
- Run benchmarks to identify performance issues
- Use the migration utilities for gradual adoption

## Conclusion

The `ShareAccountingTrait` provides a flexible, extensible foundation for share accounting in Stratum V2 implementations. By following this guide, you can:

- Migrate existing code with minimal changes
- Implement custom storage backends
- Take advantage of new features like per-user difficulty tracking
- Maintain high performance through proper optimization

The trait-based approach enables future extensibility while preserving backward compatibility with existing implementations.