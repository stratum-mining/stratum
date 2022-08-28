use async_channel::bounded;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use codec_sv2::noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey};
use roles_logic_sv2::{
    bitcoin::{secp256k1::Secp256k1, Network, PrivateKey, PublicKey},
    parsers::PoolMessages,
};
use serde::Deserialize;
use structopt::StructOpt;

mod lib;

use lib::{mining_pool::Pool, template_receiver::TemplateRx};

pub type Message = PoolMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

const HOM_GROUP_ID: u32 = u32::MAX;

const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
const NETWORK: Network = Network::Testnet;

const BLOCK_REWARD: u64 = 625_000_000_000;


fn new_pub_key() -> PublicKey {
    let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
    let secp = Secp256k1::default();
    PublicKey::from_private_key(&secp, &priv_k)
}

#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub listen_address: String,
    pub tp_address: String,
    pub authority_publib_key: EncodedEd25519PublicKey,
    pub authority_secret_key: EncodedEd25519SecretKey,
    pub cert_validity_sec: u64,
}

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(short, long, default_value = "pool-config.toml")]
    config_path: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = Args::from_args();
    let config_file = std::fs::read_to_string(args.config_path)?;
    let config: Configuration = toml::from_str(&config_file)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

    let (s_new_t, r_new_t) = bounded(10);
    let (s_prev_hash, r_prev_hash) = bounded(10);
    let (s_solution, r_solution) = bounded(10);
    println!("POOL INTITIALIZING ");
    TemplateRx::connect(config.tp_address.parse().unwrap(), s_new_t, s_prev_hash, r_solution).await;
    println!("POOL INITIALIZED");
    Pool::start(config, r_new_t, r_prev_hash, s_solution).await;
    Ok(())
}
