pub(crate) mod client;
pub(crate) mod job;
pub(crate) mod miner;
pub(crate) use client::Client;

#[async_std::main]
async fn main() {
    Client::new(80).await
}
