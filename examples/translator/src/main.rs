mod downstream_sv1;
mod error;
mod proxy;
mod upstream_sv2;

pub const UPSTREAM_IP: &str = "127.0.0.1";
pub const UPSTREAM_PORT: u16 = 34254;
pub const LISTEN_ADDR: &str = "127.0.0.1:34255";
/// TODO: Authority public key used to authorize with Upstream is hardcoded, but should be read
/// in via a proxy-config.toml.
const AUTHORITY_PUBLIC_KEY: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

#[async_std::main]
async fn main() {
    // async_std::task::spawn(async {
    proxy::Translator::initiate().await;
    // })
    // .await;
}
