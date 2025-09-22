use crate::{sniffer::*, template_provider::*};
use config_helpers_sv2::CoinbaseRewardScript;
use corepc_node::{ConnectParams, CookieValues};
use interceptor::InterceptAction;
use jd_client_sv2::JobDeclaratorClient;
use jd_server::JobDeclaratorServer;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use once_cell::sync::OnceCell;
use pool_sv2::PoolSv2;
use rand::{rng, Rng};
use std::{
    convert::{TryFrom, TryInto},
    net::SocketAddr,
};
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use translator_sv2::TranslatorSv2;
use utils::get_available_address;

pub mod interceptor;
pub mod message_aggregator;
pub mod mock_roles;
pub mod sniffer;
pub mod sniffer_error;
#[cfg(feature = "sv1")]
pub mod sv1_sniffer;
pub mod template_provider;
pub mod types;
pub(crate) mod utils;

const SHARES_PER_MINUTE: f32 = 120.0;

static LOGGER: OnceCell<()> = OnceCell::new();

/// Each test function should call `start_tracing()` to enable logging.
pub fn start_tracing() {
    LOGGER.get_or_init(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(Level::INFO.to_string()));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer())
            .init();
    });
}

pub fn start_sniffer(
    identifier: &str,
    upstream: SocketAddr,
    check_on_drop: bool,
    action: Vec<InterceptAction>,
    timeout: Option<u64>,
) -> (Sniffer<'_>, SocketAddr) {
    let listening_address = get_available_address();
    let sniffer = Sniffer::new(
        identifier,
        listening_address,
        upstream,
        check_on_drop,
        action,
        timeout,
    );
    sniffer.start();
    (sniffer, listening_address)
}

pub async fn start_pool(template_provider_address: Option<SocketAddr>) -> (PoolSv2, SocketAddr) {
    use pool_sv2::config::PoolConfig;
    let listening_address = get_available_address();
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .expect("failed");
    let cert_validity_sec = 3600;
    let coinbase_reward_script = CoinbaseRewardScript::from_descriptor(
        "wpkh(036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075)",
    )
    .unwrap();
    let pool_signature = "Stratum V2 SRI Pool".to_string();
    let tp_address = if let Some(tp_add) = template_provider_address {
        tp_add.to_string()
    } else {
        "127.0.0.1:8442".to_string()
    };
    let connection_config = pool_sv2::config::ConnectionConfig::new(
        listening_address.to_string(),
        cert_validity_sec,
        pool_signature,
    );
    let template_provider_config = pool_sv2::config::TemplateProviderConfig::new(tp_address, None);
    let authority_config =
        pool_sv2::config::AuthorityConfig::new(authority_public_key, authority_secret_key);
    let share_batch_size = 1;
    let config = PoolConfig::new(
        connection_config,
        template_provider_config,
        authority_config,
        coinbase_reward_script,
        SHARES_PER_MINUTE,
        share_batch_size,
        1,
    );
    let pool = PoolSv2::new(config);
    assert!(pool.start().await.is_ok());
    (pool, listening_address)
}

pub fn start_template_provider(
    sv2_interval: Option<u32>,
    difficulty_level: DifficultyLevel,
) -> (TemplateProvider, SocketAddr) {
    let address = get_available_address();
    let sv2_interval = sv2_interval.unwrap_or(20);
    let template_provider = TemplateProvider::start(address.port(), sv2_interval, difficulty_level);
    template_provider.generate_blocks(1);
    (template_provider, address)
}

pub fn start_jdc(
    pool: &[(SocketAddr, SocketAddr)], // (pool_address, jds_address)
    tp_address: SocketAddr,
) -> (JobDeclaratorClient, SocketAddr) {
    use jd_client_sv2::config::{
        JobDeclaratorClientConfig, PoolConfig, ProtocolConfig, TPConfig, Upstream,
    };
    let jdc_address = get_available_address();
    let max_supported_version = 2;
    let min_supported_version = 2;
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .unwrap();
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .unwrap();
    let coinbase_reward_script = CoinbaseRewardScript::from_descriptor(
        "wpkh(036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075)",
    )
    .unwrap();
    let authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .unwrap();
    let upstreams = pool
        .iter()
        .map(|(pool_addr, jds_addr)| {
            Upstream::new(
                authority_pubkey,
                pool_addr.ip().to_string(),
                pool_addr.port(),
                jds_addr.ip().to_string(),
                jds_addr.port(),
            )
        })
        .collect();
    let pool_config = PoolConfig::new(authority_public_key, authority_secret_key);
    let tp_config = TPConfig::new(1000, tp_address.to_string(), None);
    let protocol_config = ProtocolConfig::new(
        max_supported_version,
        min_supported_version,
        coinbase_reward_script,
    );
    let shares_per_minute = 10.0;
    let shares_batch_size = 1;
    let min_extranonce_size = 4;
    let user_identity = "IT-test".to_string();
    let jdc_signature = "JDC".to_string();
    let jd_client_proxy = JobDeclaratorClientConfig::new(
        jdc_address,
        protocol_config,
        user_identity,
        shares_per_minute,
        shares_batch_size,
        pool_config,
        tp_config,
        upstreams,
        jdc_signature,
        min_extranonce_size,
        None,
    );
    let ret = jd_client_sv2::JobDeclaratorClient::new(jd_client_proxy);
    let ret_clone = ret.clone();
    tokio::spawn(async move { ret_clone.start().await });
    (ret, jdc_address)
}

pub fn start_jds(tp_rpc_connection: &ConnectParams) -> (JobDeclaratorServer, SocketAddr) {
    use jd_server::config::{CoreRpc, JobDeclaratorServerConfig};
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
    let coinbase_reward_script = CoinbaseRewardScript::from_descriptor(
        "wpkh(036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075)",
    )
    .unwrap();
    if let Ok(Some(CookieValues { user, password })) = tp_rpc_connection.get_cookie_values() {
        let ip = tp_rpc_connection.rpc_socket.ip().to_string();
        let url = jd_server::Uri::builder()
            .scheme("http")
            .authority(ip)
            .path_and_query("")
            .build()
            .unwrap();
        let core_rpc = CoreRpc::new(
            url.to_string(),
            tp_rpc_connection.rpc_socket.port(),
            user,
            password,
        );
        let config = JobDeclaratorServerConfig::new(
            listen_jd_address.to_string(),
            authority_public_key,
            authority_secret_key,
            cert_validity_sec,
            coinbase_reward_script,
            core_rpc,
            std::time::Duration::from_secs(1),
        );
        let job_declarator_server = JobDeclaratorServer::new(config);
        let job_declarator_server_clone = job_declarator_server.clone();
        tokio::spawn(async move {
            job_declarator_server_clone.start().await.unwrap();
        });
        (job_declarator_server, listen_jd_address)
    } else {
        panic!("Failed to get TP cookie values");
    }
}

pub fn start_sv2_translator(upstream: SocketAddr) -> (TranslatorSv2, SocketAddr) {
    let upstream_address = upstream.ip().to_string();
    let upstream_port = upstream.port();
    let upstream_authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let listening_address = get_available_address();
    let listening_port = listening_address.port();
    let min_individual_miner_hashrate = measure_hashrate(1) as f32;

    let downstream_difficulty_config = translator_sv2::config::DownstreamDifficultyConfig::new(
        min_individual_miner_hashrate,
        SHARES_PER_MINUTE,
        true,
    );
    let upstream_conf = translator_sv2::config::Upstream::new(
        upstream_address,
        upstream_port,
        upstream_authority_pubkey,
    );
    let downstream_extranonce2_size = 4;

    let config = translator_sv2::config::TranslatorConfig::new(
        vec![upstream_conf],
        listening_address.ip().to_string(),
        listening_port,
        downstream_difficulty_config,
        2,
        2,
        downstream_extranonce2_size,
        "user_identity".to_string(),
        false,
    );
    let translator_v2 = translator_sv2::TranslatorSv2::new(config);
    let clone_translator_v2 = translator_v2.clone();
    tokio::spawn(async move {
        clone_translator_v2.start().await;
    });
    (translator_v2, listening_address)
}

pub fn measure_hashrate(duration_secs: u64) -> f64 {
    use stratum_common::roles_logic_sv2::bitcoin::hashes::{sha256d, Hash, HashEngine};

    let mut share = {
        let mut rng = rng();
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
        sha256d::Hash::from_engine(engine);
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

pub fn start_mining_device_sv1(
    upstream_addr: SocketAddr,
    single_submit: bool,
    custom_target: Option<[u8; 32]>,
) {
    tokio::spawn(async move {
        mining_device_sv1::client::Client::connect(80, upstream_addr, single_submit, custom_target)
            .await;
    });
}

pub fn start_mining_device_sv2(
    upstream: SocketAddr,
    pub_key: Option<Secp256k1PublicKey>,
    device_id: Option<String>,
    user_id: Option<String>,
    handicap: u32,
    nominal_hashrate_multiplier: Option<f32>,
    single_submit: bool,
) {
    tokio::spawn(async move {
        mining_device::connect(
            upstream.to_string(),
            pub_key,
            device_id,
            user_id,
            handicap,
            nominal_hashrate_multiplier,
            single_submit,
        )
        .await;
    });
}

#[cfg(feature = "sv1")]
pub fn start_sv1_sniffer(upstream_address: SocketAddr) -> (sv1_sniffer::SnifferSV1, SocketAddr) {
    let listening_address = get_available_address();
    let sniffer_sv1 = sv1_sniffer::SnifferSV1::new(listening_address, upstream_address);
    sniffer_sv1.start();
    (sniffer_sv1, listening_address)
}
