use async_channel::bounded;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use messages_sv2::{
    bitcoin::{secp256k1::Secp256k1, Network, PrivateKey, PublicKey},
    parsers::PoolMessages,
};

mod lib;

use lib::{mining_pool::Pool, template_receiver::TemplateRx};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

const ADDR: &str = "127.0.0.1:34254";
const TP_ADDR: &str = "127.0.0.1:8442";
const HOM_GROUP_ID: u32 = u32::MAX;

const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
const NETWORK: Network = Network::Testnet;

const BLOCK_REWARD: u64 = 625_000_000_000;

const AUTHORITY_PUBLIC_K: [u8; 32] = [
    215, 11, 47, 78, 34, 232, 25, 192, 195, 168, 170, 209, 95, 181, 40, 114, 154, 226, 176, 190,
    90, 169, 238, 89, 191, 183, 97, 63, 194, 119, 11, 31,
];

const AUTHORITY_PRIVATE_K: [u8; 32] = [
    204, 93, 167, 220, 169, 204, 172, 35, 9, 84, 174, 208, 171, 89, 25, 53, 196, 209, 161, 148, 4,
    5, 173, 0, 234, 59, 15, 127, 31, 160, 136, 131,
];

const CERT_VALIDITY: std::time::Duration = std::time::Duration::from_secs(3600);

fn new_pub_key() -> PublicKey {
    let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
    let secp = Secp256k1::default();
    PublicKey::from_private_key(&secp, &priv_k)
}

#[async_std::main]
async fn main() {
    //let test: bool = std::env::var("TEST").unwrap().parse().unwrap();
    let test = false;
    let (s_new_t, r_new_t) = bounded(10);
    let (s_prev_hash, r_prev_hash) = bounded(10);
    let (s_solution, r_solution) = bounded(10);
    if test {
        crate::lib::template_receiver::test_template::TestTemplateRx::start(s_new_t, s_prev_hash)
            .await;
    } else {
        TemplateRx::connect(TP_ADDR.parse().unwrap(), s_new_t, s_prev_hash, r_solution).await;
    }
    Pool::start(r_new_t, r_prev_hash, s_solution).await;
}
