#![allow(special_module_name)]
#![allow(clippy::option_map_unit_fn)]
use key_utils::Secp256k1PublicKey;

use clap::Parser;
use tracing::info;

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
    #[arg(
        long,
        help = "This floating point number is used to modify the advertised nominal hashrate when opening a channel with the upstream.\
         \nIf 0.0 < nominal_hashrate_multiplier < 1.0, the CPU miner will advertise a nominal hashrate that is smaller than its real capacity.\
         \nIf nominal_hashrate_multiplier > 1.0, the CPU miner will advertise a nominal hashrate that is bigger than its real capacity.\
         \nIf empty, the CPU miner will simply advertise its real capacity."
    )]
    nominal_hashrate_multiplier: Option<f32>,
    #[arg(
        long,
        help = "Number of nonces to try per mining loop iteration when fast hashing is available (micro-batching)",
        default_value = "32"
    )]
    nonces_per_call: u32,
    #[arg(
        long,
        help = "Number of worker threads to use for mining. Defaults to logical CPUs minus one (leaves one core free)."
    )]
    cores: Option<u32>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt::init();
    info!("start");
    // Configure micro-batch size
    mining_device::set_nonces_per_call(args.nonces_per_call);
    // Optional override of worker threads
    if let Some(n) = args.cores {
        mining_device::set_cores(n);
    }
    // Log worker usage (after applying overrides)
    let used = mining_device::effective_worker_count();
    let total = mining_device::total_logical_cpus();
    info!(
        "Using {} worker threads out of {} logical CPUs",
        used, total
    );
    let _ = mining_device::connect(
        args.address_pool,
        args.pubkey_pool,
        args.id_device,
        args.id_user,
        args.handicap,
        args.nominal_hashrate_multiplier,
        false,
    )
    .await;
}
