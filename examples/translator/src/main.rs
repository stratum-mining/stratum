mod downstream;
mod proxy;
mod upstream;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

pub const LISTEN_ADDR: &str = "127.0.0.1:34255";

#[async_std::main]
async fn main() {
    let authority_public_key = [
        215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176,
        190, 90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
    ];
    let upstream_addr = SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 34254);
    let _upstream = upstream::Upstream::new(upstream_addr, authority_public_key).await;
    async_std::task::spawn(async {
        downstream::listen_downstream().await;
    })
    .await;
}
