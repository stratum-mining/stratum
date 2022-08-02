mod downstream;

pub const LISTEN_ADDR: &str = "127.0.0.1:34254";

#[async_std::main]
async fn main() {
    async_std::task::spawn(async {
        downstream::listen_downstream().await;
    }).await;
}
