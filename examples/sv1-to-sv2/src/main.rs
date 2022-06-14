use async_std::task;
// use mining_sv2;

mod sv1_downstream;
mod sv2_upstream;

/// Sv1 Downstream (Miner) <-> Sv1/Sv2 Proxy <-> Sv2 Upstream (Pool)
fn main() {
    println!("Hello, sv1 to sv2 translator!");
    // This is still sv1
    std::thread::spawn(|| {
        task::spawn(async {
            sv2_upstream::server_pool().await;
        });
    });
    task::block_on(async {
        let client = sv1_downstream::Client::new(80).await;
        sv1_downstream::initialize_client(client).await;
    });
}
