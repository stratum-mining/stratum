mod downstream_sv1;
mod proxy;
mod upstream_sv2;

pub const LISTEN_ADDR: &str = "127.0.0.1:34255";

#[async_std::main]
async fn main() {
    async_std::task::spawn(async {
        let _ = proxy::Translator::new().await;
    })
    .await;
}
