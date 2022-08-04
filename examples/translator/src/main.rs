mod downstream_sv1;
mod proxy;
mod upstream_sv2;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

pub const LISTEN_ADDR: &str = "127.0.0.1:34255";

#[async_std::main]
async fn main() {
    let translator = proxy::Translator::new().await;
    let authority_public_key = [
        215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176,
        190, 90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
    ];
    let upstream_addr = SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 34254);
    let _upstream = upstream_sv2::Upstream::new(
        upstream_addr,
        authority_public_key,
        translator.sender_upstream.clone(),
        translator.receiver_upstream.clone(),
    )
    .await
    .unwrap();
    let sender_downstream_clone = translator.sender_downstream.clone();
    let receiver_downstream_clone = translator.receiver_downstream.clone();
    async_std::task::spawn(async {
        downstream_sv1::listen_downstream(sender_downstream_clone, receiver_downstream_clone).await;
    })
    .await;
}
