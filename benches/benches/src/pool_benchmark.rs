use async_std::net::TcpListener;
use async_std::{prelude::*, task};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use psutil::cpu::cpu_times_percpu;
use psutil::cpu::os::unix::CpuTimesExt;
use psutil::memory::virtual_memory;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

#[path = "./lib/server.rs"]
mod server;
use crate::server::*;

#[path = "./lib/client.rs"]
mod client;
use crate::client::*;

async fn server_pool_listen(listener: TcpListener, num_connections: usize) {
    let mut incoming = listener.incoming();
    let mut connection_count = 0;

    while let Some(stream) = incoming.next().await {
        let stream = stream.unwrap();
        Server::new(stream).await;
        connection_count += 1;
        if connection_count >= num_connections {
            break;
        }
    }
}

fn calculate_cpu_usage(cpu_times: &[psutil::cpu::CpuTimes]) -> f64 {
    let total_cpu_time: f64 = cpu_times
        .iter()
        .map(|cpu| {
            cpu.user().as_secs_f64()
                + cpu.system().as_secs_f64()
                + cpu.idle().as_secs_f64()
                + cpu.nice().as_secs_f64()
        })
        .sum();

    let idle_cpu_time: f64 = cpu_times.iter().map(|cpu| cpu.idle().as_secs_f64()).sum();

    let cpu_usage = 100.0 - (idle_cpu_time / total_cpu_time) * 100.0;
    cpu_usage
}

fn server_client_benchmark(c: &mut Criterion) {
    c.bench_function("server_client_benchmark", |b| {
        b.iter_custom(|iters| {
            let mut total_latency = 0.0;
            let mut total_cpu_usage = 0.0;
            let mut total_memory_usage = 0.0;

            let mut file = File::create("benchmark_results.txt").unwrap();
			let start = Instant::now();
            for _ in 0..iters {
                task::block_on(async {
                    let listener = TcpListener::bind("127.0.0.1:3333").await.unwrap();
                    let socket = listener.local_addr().unwrap();
                    task::spawn(async move {
                        server_pool_listen(listener, 1).await;
                    });

                    let start = Instant::now();

                    let client = Client::new(80, socket).await;
                    initialize_client(client).await;

                    let elapsed = start.elapsed();
                    let latency = elapsed.as_secs_f64();

                    let cpu_usage = cpu_times_percpu().unwrap();
                    let average_cpu_usage = calculate_cpu_usage(&cpu_usage);
                    let memory_usage = virtual_memory().unwrap().percent();

                    println!( "Latency: {} seconds", latency);
                    println!( "CPU Usage: {:.2}%", average_cpu_usage);
                    println!( "Memory Usage: {:.2}%", memory_usage);
                    println!( "Instructions per Cycle: Not available yet");

                    total_latency += latency;
                    total_cpu_usage += average_cpu_usage;
                    total_memory_usage += memory_usage;

                    // Use black_box to prevent compiler optimizations
                    black_box(latency);
                    black_box(average_cpu_usage);
                    black_box(memory_usage);
                });
            }

            let average_latency = total_latency / (iters as f64);
            let average_cpu_usage = total_cpu_usage / (iters as f64);
            let average_memory_usage = total_memory_usage / (iters as f32);

            writeln!(file, "").unwrap();
            writeln!(file, "Average Latency: {} seconds", average_latency).unwrap();
            writeln!(file, "Average CPU Usage: {:.2}%", average_cpu_usage).unwrap();
            writeln!(file, "Average Memory Usage: {:.2}%", average_memory_usage).unwrap();

            format!(
                "Average Latency: {} seconds\nAverage CPU Usage: {:.2}%\nAverage Memory Usage: {:.2}%",
                average_latency, average_cpu_usage, average_memory_usage
            );

			let elapsed = start.elapsed();
			elapsed
        });

    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = server_client_benchmark
}
criterion_main!(benches);
