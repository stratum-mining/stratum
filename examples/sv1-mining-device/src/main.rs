pub mod client;
pub use client::Client;

#[async_std::main]
async fn main() {
    Client::new(80).await
}
