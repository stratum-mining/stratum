//! Performance benchmarks comparing old concrete ShareAccounting implementations
//! with the new trait-based approach.
//!
//! These benchmarks verify that the trait abstraction doesn't introduce significant
//! performance overhead compared to the original concrete implementations.

use bitcoin::hashes::sha256d::Hash;
use bitcoin::hashes::Hash as HashTrait;
use channels_sv2::client::share_accounting::ShareAccounting as ClientShareAccounting;
use channels_sv2::server::share_accounting::ShareAccounting as ServerShareAccounting;
use channels_sv2::share_accounting_trait::{
    create_client_share_accounting, create_server_share_accounting, ShareAccountingTrait,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

/// Generate a deterministic hash for benchmarking
fn generate_hash(seed: u8) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    Hash::from_slice(&bytes).unwrap()
}

/// Benchmark share processing performance for client implementations
fn bench_client_share_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_share_processing");
    
    // Test with different numbers of shares
    for share_count in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*share_count as u64));
        
        // Benchmark original client implementation
        group.bench_with_input(
            BenchmarkId::new("original_client", share_count),
            share_count,
            |b, &share_count| {
                b.iter(|| {
                    let mut accounting = ClientShareAccounting::new();
                    for i in 0..share_count {
                        let hash = generate_hash((i % 256) as u8);
                        accounting.update_share_accounting(
                            black_box(1000 + i as u64),
                            black_box(i as u32 + 1),
                            black_box(hash),
                        );
                    }
                    black_box(accounting)
                });
            },
        );
        
        // Benchmark trait-based client implementation
        group.bench_with_input(
            BenchmarkId::new("trait_client", share_count),
            share_count,
            |b, &share_count| {
                b.iter(|| {
                    let mut accounting = create_client_share_accounting();
                    for i in 0..share_count {
                        let hash = generate_hash((i % 256) as u8);
                        accounting
                            .update_share_accounting(
                                black_box(1000 + i as u64),
                                black_box(i as u32 + 1),
                                black_box(hash),
                            )
                            .unwrap();
                    }
                    black_box(accounting)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark share processing performance for server implementations
fn bench_server_share_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("server_share_processing");
    
    // Test with different numbers of shares
    for share_count in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*share_count as u64));
        
        // Benchmark original server implementation
        group.bench_with_input(
            BenchmarkId::new("original_server", share_count),
            share_count,
            |b, &share_count| {
                b.iter(|| {
                    let mut accounting = ServerShareAccounting::new(10);
                    for i in 0..share_count {
                        let hash = generate_hash((i % 256) as u8);
                        accounting.update_share_accounting(
                            black_box(1000 + i as u64),
                            black_box(i as u32 + 1),
                            black_box(hash),
                        );
                    }
                    black_box(accounting)
                });
            },
        );
        
        // Benchmark trait-based server implementation
        group.bench_with_input(
            BenchmarkId::new("trait_server", share_count),
            share_count,
            |b, &share_count| {
                b.iter(|| {
                    let mut accounting = create_server_share_accounting(10);
                    for i in 0..share_count {
                        let hash = generate_hash((i % 256) as u8);
                        accounting
                            .update_share_accounting(
                                black_box(1000 + i as u64),
                                black_box(i as u32 + 1),
                                black_box(hash),
                            )
                            .unwrap();
                    }
                    black_box(accounting)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark duplicate detection performance
fn bench_duplicate_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("duplicate_detection");
    
    // Pre-generate hashes for consistent benchmarking
    let hashes: Vec<Hash> = (0..1000).map(|i| generate_hash((i % 256) as u8)).collect();
    
    // Benchmark original client duplicate detection
    group.bench_function("original_client_duplicate_check", |b| {
        b.iter(|| {
            let mut accounting = ClientShareAccounting::new();
            
            // Add some shares first
            for (i, &hash) in hashes.iter().take(500).enumerate() {
                accounting.update_share_accounting(1000, i as u32 + 1, hash);
            }
            
            // Check for duplicates
            let mut duplicate_count = 0;
            for &hash in &hashes {
                if accounting.is_share_seen(black_box(hash)) {
                    duplicate_count += 1;
                }
            }
            black_box(duplicate_count)
        });
    });
    
    // Benchmark trait-based duplicate detection
    group.bench_function("trait_client_duplicate_check", |b| {
        b.iter(|| {
            let mut accounting = create_client_share_accounting();
            
            // Add some shares first
            for (i, &hash) in hashes.iter().take(500).enumerate() {
                accounting
                    .update_share_accounting(1000, i as u32 + 1, hash)
                    .unwrap();
            }
            
            // Check for duplicates
            let mut duplicate_count = 0;
            for &hash in &hashes {
                if accounting.is_share_seen(black_box(hash)).unwrap() {
                    duplicate_count += 1;
                }
            }
            black_box(duplicate_count)
        });
    });
    
    group.finish();
}

/// Benchmark batch acknowledgment logic
fn bench_batch_acknowledgment(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_acknowledgment");
    
    // Benchmark original server acknowledgment logic
    group.bench_function("original_server_acknowledgment", |b| {
        b.iter(|| {
            let mut accounting = ServerShareAccounting::new(10);
            let mut acknowledgment_count = 0;
            
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                accounting.update_share_accounting(1000, i + 1, hash);
                
                if accounting.should_acknowledge() {
                    acknowledgment_count += 1;
                }
            }
            
            black_box(acknowledgment_count)
        });
    });
    
    // Benchmark trait-based acknowledgment logic
    group.bench_function("trait_server_acknowledgment", |b| {
        b.iter(|| {
            let mut accounting = create_server_share_accounting(10);
            let mut acknowledgment_count = 0;
            
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                accounting
                    .update_share_accounting(1000, i + 1, hash)
                    .unwrap();
                
                if accounting.should_acknowledge().unwrap() {
                    acknowledgment_count += 1;
                }
            }
            
            black_box(acknowledgment_count)
        });
    });
    
    group.finish();
}

/// Benchmark difficulty tracking performance
fn bench_difficulty_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("difficulty_tracking");
    
    // Benchmark original client difficulty tracking
    group.bench_function("original_client_difficulty", |b| {
        b.iter(|| {
            let mut accounting = ClientShareAccounting::new();
            
            for i in 0..1000 {
                let difficulty = 1000.0 + (i as f64 * 0.1);
                accounting.update_best_diff(black_box(difficulty));
            }
            
            black_box(accounting.get_best_diff())
        });
    });
    
    // Benchmark trait-based difficulty tracking
    group.bench_function("trait_client_difficulty", |b| {
        b.iter(|| {
            let mut accounting = create_client_share_accounting();
            
            for i in 0..1000 {
                let difficulty = 1000.0 + (i as f64 * 0.1);
                accounting.update_best_diff(black_box(difficulty)).unwrap();
            }
            
            black_box(accounting.get_best_diff().unwrap())
        });
    });
    
    // Benchmark per-user difficulty tracking (new feature)
    group.bench_function("trait_per_user_difficulty", |b| {
        b.iter(|| {
            let mut accounting = create_client_share_accounting();
            
            // Simulate multiple users
            for user_id in 0..100 {
                let user_name = format!("user_{}", user_id);
                for i in 0..10 {
                    let difficulty = 1000.0 + (user_id as f64 * 10.0) + (i as f64 * 0.1);
                    accounting
                        .update_best_diff_for_user(&user_name, black_box(difficulty))
                        .unwrap();
                }
            }
            
            // Get top users
            let top_users = accounting.get_top_user_difficulties(10).unwrap();
            black_box(top_users)
        });
    });
    
    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_operations");
    
    // Benchmark cache flush performance
    group.bench_function("original_client_flush", |b| {
        b.iter(|| {
            let mut accounting = ClientShareAccounting::new();
            
            // Fill with shares
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                accounting.update_share_accounting(1000, i + 1, hash);
            }
            
            // Flush cache
            accounting.flush_seen_shares();
            black_box(accounting)
        });
    });
    
    group.bench_function("trait_client_flush", |b| {
        b.iter(|| {
            let mut accounting = create_client_share_accounting();
            
            // Fill with shares
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                accounting
                    .update_share_accounting(1000, i + 1, hash)
                    .unwrap();
            }
            
            // Flush cache
            accounting.flush_seen_shares().unwrap();
            black_box(accounting)
        });
    });
    
    group.finish();
}

/// Comprehensive benchmark comparing all operations
fn bench_comprehensive_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("comprehensive_workload");
    
    // Simulate a realistic workload with mixed operations
    group.bench_function("original_server_workload", |b| {
        b.iter(|| {
            let mut accounting = ServerShareAccounting::new(50);
            let mut acknowledgments = 0;
            
            // Process 1000 shares with mixed operations
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                
                // Check for duplicate (some will be duplicates)
                if !accounting.is_share_seen(hash) {
                    // Process new share
                    accounting.update_share_accounting(1000 + i as u64, i + 1, hash);
                    
                    // Update difficulty occasionally
                    if i % 10 == 0 {
                        accounting.update_best_diff(1000.0 + (i as f64 * 0.1));
                    }
                    
                    // Check for acknowledgment
                    if accounting.should_acknowledge() {
                        acknowledgments += 1;
                    }
                }
                
                // Flush cache occasionally (simulate chain tip updates)
                if i % 100 == 0 {
                    accounting.flush_seen_shares();
                }
            }
            
            black_box((acknowledgments, accounting.get_best_diff()))
        });
    });
    
    group.bench_function("trait_server_workload", |b| {
        b.iter(|| {
            let mut accounting = create_server_share_accounting(50);
            let mut acknowledgments = 0;
            
            // Process 1000 shares with mixed operations
            for i in 0..1000 {
                let hash = generate_hash((i % 256) as u8);
                
                // Check for duplicate (some will be duplicates)
                if !accounting.is_share_seen(hash).unwrap() {
                    // Process new share
                    accounting
                        .update_share_accounting(1000 + i as u64, i + 1, hash)
                        .unwrap();
                    
                    // Update difficulty occasionally
                    if i % 10 == 0 {
                        accounting
                            .update_best_diff(1000.0 + (i as f64 * 0.1))
                            .unwrap();
                    }
                    
                    // Check for acknowledgment
                    if accounting.should_acknowledge().unwrap() {
                        acknowledgments += 1;
                    }
                }
                
                // Flush cache occasionally (simulate chain tip updates)
                if i % 100 == 0 {
                    accounting.flush_seen_shares().unwrap();
                }
            }
            
            black_box((acknowledgments, accounting.get_best_diff().unwrap()))
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_client_share_processing,
    bench_server_share_processing,
    bench_duplicate_detection,
    bench_batch_acknowledgment,
    bench_difficulty_tracking,
    bench_memory_operations,
    bench_comprehensive_workload
);

criterion_main!(benches);