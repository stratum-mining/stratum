mod sniffer;

use bitcoind::{bitcoincore_rpc::RpcApi, BitcoinD, Conf};
use flate2::read::GzDecoder;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use once_cell::sync::Lazy;
use pool_sv2::PoolSv2;
use rand::{thread_rng, Rng};
use sniffer::Sniffer;
pub use sniffer::{InterceptMessage, MessageDirection};
use std::{
    collections::HashSet,
    convert::{TryFrom, TryInto},
    env,
    fs::{create_dir_all, File},
    io::{BufReader, Read},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Mutex,
};
use tar::Archive;

// prevents get_available_port from ever returning the same port twice
static UNIQUE_PORTS: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

const VERSION_TP: &str = "0.1.9";

fn download_bitcoind_tarball(download_url: &str) -> Vec<u8> {
    let response = minreq::get(download_url)
        .send()
        .unwrap_or_else(|_| panic!("Cannot reach URL: {}", download_url));
    assert_eq!(
        response.status_code, 200,
        "URL {} didn't return 200",
        download_url
    );
    response.as_bytes().to_vec()
}

fn read_tarball_from_file(path: &str) -> Vec<u8> {
    let file = File::open(path).unwrap_or_else(|_| {
        panic!(
            "Cannot find {:?} specified with env var BITCOIND_TARBALL_FILE",
            path
        )
    });
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();
    buffer
}

fn unpack_tarball(tarball_bytes: &[u8], destination: &Path) {
    let decoder = GzDecoder::new(tarball_bytes);
    let mut archive = Archive::new(decoder);
    for mut entry in archive.entries().unwrap().flatten() {
        if let Ok(file) = entry.path() {
            if file.ends_with("bitcoind") {
                entry.unpack_in(destination).unwrap();
            }
        }
    }
}

fn get_bitcoind_filename(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => format!("bitcoin-sv2-tp-{}-arm64-apple-darwin.tar.gz", VERSION_TP),
        ("macos", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-apple-darwin.tar.gz", VERSION_TP),
        ("linux", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-linux-gnu.tar.gz", VERSION_TP),
        ("linux", "aarch64") => format!("bitcoin-sv2-tp-{}-aarch64-linux-gnu.tar.gz", VERSION_TP),
        _ => format!(
            "bitcoin-sv2-tp-{}-x86_64-apple-darwin-unsigned.zip",
            VERSION_TP
        ),
    }
}

pub struct TemplateProvider {
    bitcoind: BitcoinD,
}

impl TemplateProvider {
    pub fn start(port: u16) -> Self {
        let temp_dir = PathBuf::from("/tmp/.template-provider");
        let mut conf = Conf::default();
        let staticdir = format!(".bitcoin-{}", port);
        conf.staticdir = Some(temp_dir.join(staticdir));
        let port = format!("-sv2port={}", port);
        conf.args.extend(vec![
            "-txindex=1",
            "-sv2",
            &port,
            "-debug=sv2",
            "-sv2interval=20",
            "-sv2feedelta=1000",
            "-loglevel=sv2:trace",
        ]);

        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let download_filename = get_bitcoind_filename(os, arch);
        let bitcoin_exe_home = temp_dir
            .join(format!("bitcoin-sv2-tp-{}", VERSION_TP))
            .join("bin");

        if !bitcoin_exe_home.exists() {
            let tarball_bytes = match env::var("BITCOIND_TARBALL_FILE") {
                Ok(path) => read_tarball_from_file(&path),
                Err(_) => {
                    let download_endpoint =
                        env::var("BITCOIND_DOWNLOAD_ENDPOINT").unwrap_or_else(|_| {
                            "https://github.com/Sjors/bitcoin/releases/download".to_owned()
                        });
                    let url = format!(
                        "{}/sv2-tp-{}/{}",
                        download_endpoint, VERSION_TP, download_filename
                    );
                    download_bitcoind_tarball(&url)
                }
            };

            if let Some(parent) = bitcoin_exe_home.parent() {
                create_dir_all(parent).unwrap();
            }

            unpack_tarball(&tarball_bytes, &temp_dir);

            if os == "macos" {
                let bitcoind_binary = bitcoin_exe_home.join("bitcoind");
                std::process::Command::new("codesign")
                    .arg("--sign")
                    .arg("-")
                    .arg(&bitcoind_binary)
                    .output()
                    .expect("Failed to sign bitcoind binary");
            }
        }

        env::set_var("BITCOIND_EXE", bitcoin_exe_home.join("bitcoind"));
        let exe_path = bitcoind::exe_path().unwrap();

        let bitcoind = BitcoinD::with_conf(exe_path, &conf).unwrap();

        TemplateProvider { bitcoind }
    }

    fn stop(&self) {
        let _ = self.bitcoind.client.stop().unwrap();
    }

    fn generate_blocks(&self, n: u64) {
        let mining_address = self
            .bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .require_network(bitcoind::bitcoincore_rpc::bitcoin::Network::Regtest)
            .unwrap();
        self.bitcoind
            .client
            .generate_to_address(n, &mining_address)
            .unwrap();
    }
}

impl Drop for TemplateProvider {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_port_open(address: SocketAddr) -> bool {
    TcpListener::bind(address).is_err()
}

fn get_available_port() -> u16 {
    let mut unique_ports = UNIQUE_PORTS.lock().unwrap();

    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if !unique_ports.contains(&port) {
            unique_ports.insert(port);
            return port;
        }
    }
}

pub fn get_available_address() -> SocketAddr {
    let port = get_available_port();
    SocketAddr::from(([127, 0, 0, 1], port))
}

pub async fn start_sniffer(
    identifier: String,
    listening_address: SocketAddr,
    upstream: SocketAddr,
    check_on_drop: bool,
    intercept_message: Option<Vec<InterceptMessage>>,
) -> Sniffer {
    let sniffer = Sniffer::new(
        identifier,
        listening_address,
        upstream,
        check_on_drop,
        intercept_message,
    )
    .await;
    let sniffer_clone = sniffer.clone();
    tokio::spawn(async move {
        sniffer_clone.start().await;
    });
    sniffer
}

#[derive(Debug)]
struct TestPoolSv2 {
    pool: PoolSv2,
}

impl TestPoolSv2 {
    fn new(
        listening_address: Option<SocketAddr>,
        template_provider_address: Option<SocketAddr>,
    ) -> Self {
        use pool_sv2::mining_pool::{CoinbaseOutput, Configuration};
        let pool_port = if let Some(listen_addr) = listening_address {
            listen_addr.port()
        } else {
            get_available_port()
        };
        let listening_address = listening_address
            .unwrap_or(SocketAddr::from_str(&format!("127.0.0.1:{}", pool_port)).unwrap());
        let is_pool_port_open = is_port_open(listening_address);
        assert!(!is_pool_port_open);
        let authority_public_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let authority_secret_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");
        let cert_validity_sec = 3600;
        let coinbase_outputs = vec![CoinbaseOutput::new(
            "P2WPKH".to_string(),
            "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
        )];
        let pool_signature = "Stratum v2 SRI Pool".to_string();
        let tp_address = if let Some(tp_add) = template_provider_address {
            tp_add.to_string()
        } else {
            "127.0.0.1:8442".to_string()
        };
        let connection_config = pool_sv2::mining_pool::ConnectionConfig::new(
            listening_address.to_string(),
            cert_validity_sec,
            pool_signature,
        );
        let template_provider_config =
            pool_sv2::mining_pool::TemplateProviderConfig::new(tp_address, None);
        let authority_config =
            pool_sv2::mining_pool::AuthorityConfig::new(authority_public_key, authority_secret_key);
        let config = Configuration::new(
            connection_config,
            template_provider_config,
            authority_config,
            coinbase_outputs,
        );
        let pool = PoolSv2::new(config);

        Self { pool }
    }
}

pub async fn start_pool(
    listening_address: Option<SocketAddr>,
    template_provider_address: Option<SocketAddr>,
) -> PoolSv2 {
    let test_pool = TestPoolSv2::new(listening_address, template_provider_address);
    let pool = test_pool.pool.clone();
    let pool_clone = pool.clone();
    tokio::task::spawn(async move {
        assert!(pool_clone.start().await.is_ok());
    });
    // Wait a bit to let the pool exchange initial messages with the TP
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    pool
}

pub async fn start_template_provider(tp_port: u16) -> TemplateProvider {
    let template_provider = TemplateProvider::start(tp_port);
    template_provider.generate_blocks(16);
    template_provider
}

pub async fn start_jdc(
    pool_address: SocketAddr,
    tp_address: SocketAddr,
    jds_address: SocketAddr,
) -> SocketAddr {
    use jd_client::proxy_config::{
        CoinbaseOutput, PoolConfig, ProtocolConfig, ProxyConfig, TPConfig, Upstream,
    };
    let jdc_address = get_available_address();
    let max_supported_version = 2;
    let min_supported_version = 2;
    let min_extranonce2_size = 8;
    let withhold = false;
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .unwrap();
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .unwrap();
    let cert_validity_sec = 3600;
    let coinbase_outputs = vec![CoinbaseOutput::new(
        "P2WPKH".to_string(),
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
    )];
    let authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .unwrap();
    let pool_signature = "Stratum v2 SRI Pool".to_string();
    let upstreams = vec![Upstream::new(
        authority_pubkey,
        pool_address.to_string(),
        jds_address.to_string(),
        pool_signature,
    )];
    let pool_config = PoolConfig::new(authority_public_key, authority_secret_key);
    let tp_config = TPConfig::new(1000, tp_address.to_string(), None);
    let protocol_config = ProtocolConfig::new(
        max_supported_version,
        min_supported_version,
        min_extranonce2_size,
        coinbase_outputs,
    );
    let jd_client_proxy = ProxyConfig::new(
        jdc_address,
        protocol_config,
        withhold,
        pool_config,
        tp_config,
        upstreams,
        std::time::Duration::from_secs(cert_validity_sec),
    );
    let ret = jd_client::JobDeclaratorClient::new(jd_client_proxy);
    tokio::spawn(async move { ret.start().await });
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    jdc_address
}

pub async fn start_jds(tp_address: SocketAddr) -> SocketAddr {
    use jd_server::{CoinbaseOutput, Configuration, CoreRpc, JobDeclaratorServer};
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .unwrap();
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .unwrap();
    let listen_jd_address = get_available_address();
    let cert_validity_sec = 3600;
    let coinbase_outputs = vec![CoinbaseOutput::new(
        "P2WPKH".to_string(),
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
    )];
    let core_rpc = CoreRpc::new(
        tp_address.ip().to_string(),
        tp_address.port(),
        "tp_username".to_string(),
        "tp_password".to_string(),
    );
    let config = Configuration::new(
        listen_jd_address.to_string(),
        authority_public_key,
        authority_secret_key,
        cert_validity_sec,
        coinbase_outputs,
        core_rpc,
        std::time::Duration::from_secs(1),
    );
    tokio::spawn(async move {
        JobDeclaratorServer::new(config).start().await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    listen_jd_address
}

pub async fn start_sv2_translator(upstream: SocketAddr) -> SocketAddr {
    let upstream_address = upstream.ip().to_string();
    let upstream_port = upstream.port();
    let upstream_authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let listening_address = get_available_address();
    let listening_port = listening_address.port();
    let hashrate = measure_hashrate(3) as f32 / 20.0;
    let min_individual_miner_hashrate = hashrate;
    let shares_per_minute = 100.0;
    let channel_diff_update_interval = 60;
    let channel_nominal_hashrate = hashrate;
    let downstream_difficulty_config =
        translator_sv2::proxy_config::DownstreamDifficultyConfig::new(
            min_individual_miner_hashrate,
            shares_per_minute,
            0,
            0,
        );
    let upstream_difficulty_config = translator_sv2::proxy_config::UpstreamDifficultyConfig::new(
        channel_diff_update_interval,
        channel_nominal_hashrate,
        0,
        false,
    );
    let upstream_conf = translator_sv2::proxy_config::UpstreamConfig::new(
        upstream_address,
        upstream_port,
        upstream_authority_pubkey,
        upstream_difficulty_config,
    );
    let downstream_conf = translator_sv2::proxy_config::DownstreamConfig::new(
        listening_address.ip().to_string(),
        listening_port,
        downstream_difficulty_config,
    );

    let config =
        translator_sv2::proxy_config::ProxyConfig::new(upstream_conf, downstream_conf, 2, 2, 8);
    let translator_v2 = translator_sv2::TranslatorSv2::new(config);
    tokio::spawn(async move {
        translator_v2.start().await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    listening_address
}

fn measure_hashrate(duration_secs: u64) -> f64 {
    use stratum_common::bitcoin::hashes::{sha256d, Hash, HashEngine};

    let mut share = {
        let mut rng = thread_rng();
        let mut arr = [0u8; 80];
        rng.fill(&mut arr[..]);
        arr
    };
    let start_time = std::time::Instant::now();
    let mut hashes: u64 = 0;
    let duration = std::time::Duration::from_secs(duration_secs);

    let hash = |share: &mut [u8; 80]| {
        let nonce: [u8; 8] = share[0..8].try_into().unwrap();
        let mut nonce = u64::from_le_bytes(nonce);
        nonce += 1;
        share[0..8].copy_from_slice(&nonce.to_le_bytes());
        let mut engine = sha256d::Hash::engine();
        engine.input(share);
        sha256d::Hash::from_engine(engine).into_inner();
    };

    loop {
        if start_time.elapsed() >= duration {
            break;
        }
        hash(&mut share);
        hashes += 1;
    }

    let elapsed_secs = start_time.elapsed().as_secs_f64();

    hashes as f64 / elapsed_secs
}

pub async fn start_mining_device_sv1(upstream_addr: SocketAddr) {
    tokio::spawn(async move {
        mining_device_sv1::client::Client::connect(80, upstream_addr).await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}
