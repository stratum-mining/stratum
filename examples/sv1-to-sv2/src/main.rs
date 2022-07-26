use tokio::net::TcpStream;
const ADDR: &str = "127.0.0.1:34254";
// use mining_sv2;

// mod sv1_downstream;
// mod sv2_upstream;

/// Sv1 Downstream (Miner) <-> Sv1/Sv2 Proxy <-> Sv2 Upstream (Pool)
#[tokio::main]
async fn main() {
    println!("Hello, sv1 to sv2 translator!");
    // Wait for upstream
    let stream = TcpStream::connect(ADDR).await.unwrap();

    // // This is still sv1
    // std::thread::spawn(|| {
    //     task::spawn(async {
    //         sv2_upstream::server_pool().await;
    //     });
    // });
    // task::block_on(async {
    //     let client = sv1_downstream::Client::new(80).await;
    //     sv1_downstream::initialize_client(client).await;
    // });
}
