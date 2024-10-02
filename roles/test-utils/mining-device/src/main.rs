#![allow(special_module_name)]
#![allow(clippy::option_map_unit_fn)]
use key_utils::Secp256k1PublicKey;

use clap::Parser;
use tracing::info;

pub mod lib;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        help = "Pool pub key, when left empty the pool certificate is not checked"
    )]
    pubkey_pool: Option<Secp256k1PublicKey>,
    #[arg(
        short,
        long,
        help = "Sometimes used by the pool to identify the device"
    )]
    id_device: Option<String>,
    #[arg(
        short,
        long,
        help = "Address of the pool in this format ip:port or domain:port"
    )]
    address_pool: String,
    #[arg(
        long,
        help = "This value is used to slow down the cpu miner, it represents the number of micro-seconds that are awaited between hashes",
        default_value = "0"
    )]
    handicap: u32,
    #[arg(
        long,
        help = "User id, used when a new channel is opened, it can be used by the pool to identify the miner"
    )]
    id_user: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt::init();
    info!("start");
    let _ = lib::connect(
        args.address_pool,
        args.pubkey_pool,
        args.id_device,
        args.id_user,
        args.handicap,
    )
    .await;
}
