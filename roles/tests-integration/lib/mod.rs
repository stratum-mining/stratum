use crate::{sniffer::*, template_provider::*};
use jd_client::JobDeclaratorClient;
use jd_server::JobDeclaratorServer;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use pool_sv2::PoolSv2;
use translator_sv2::TranslatorSv2;

use rand::{thread_rng, Rng};
use std::{
    convert::{TryFrom, TryInto},
    net::SocketAddr,
    str::FromStr,
};
use utils::get_available_address;

pub mod sniffer;
pub mod template_provider;
mod utils;

pub async fn start_sniffer(
    identifier: String,
    upstream: SocketAddr,
    check_on_drop: bool,
    intercept_message: Option<Vec<sniffer::InterceptMessage>>,
) -> (Sniffer, SocketAddr) {
    let listening_address = get_available_address();
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
    (sniffer, listening_address)
}

pub async fn start_pool(template_provider_address: Option<SocketAddr>) -> (PoolSv2, SocketAddr) {
    use pool_sv2::mining_pool::{CoinbaseOutput, Configuration};
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
    let pool_clone = pool.clone();
    tokio::task::spawn(async move {
        assert!(pool_clone.start().await.is_ok());
    });
    // Wait a bit to let the pool exchange initial messages with the TP
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    (pool, listening_address)
}

pub async fn start_template_provider(sv2_interval: Option<u32>) -> (TemplateProvider, SocketAddr) {
    let address = get_available_address();
    let sv2_interval = sv2_interval.unwrap_or(20);
    let template_provider = TemplateProvider::start(address.port(), sv2_interval);
    template_provider.generate_blocks(16);
    (template_provider, address)
}

pub async fn start_jdc(
    pool_address: SocketAddr,
    tp_address: SocketAddr,
    jds_address: SocketAddr,
) -> (JobDeclaratorClient, SocketAddr) {
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
    let ret_clone = ret.clone();
    tokio::spawn(async move { ret_clone.start().await });
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    (ret, jdc_address)
}

pub async fn start_jds(tp_address: SocketAddr) -> (JobDeclaratorServer, SocketAddr) {
    use jd_server::{CoinbaseOutput, Configuration, CoreRpc};
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
    let job_declarator_server = JobDeclaratorServer::new(config);
    let job_declarator_server_clone = job_declarator_server.clone();
    tokio::spawn(async move {
        job_declarator_server_clone.start().await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    (job_declarator_server, listen_jd_address)
}

pub async fn start_sv2_translator(upstream: SocketAddr) -> (TranslatorSv2, SocketAddr) {
    let upstream_address = upstream.ip().to_string();
    let upstream_port = upstream.port();
    let upstream_authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let listening_address = get_available_address();
    let listening_port = listening_address.port();
    let hashrate = measure_hashrate(1) as f32 / 100.0;
    let min_individual_miner_hashrate = hashrate;
    let shares_per_minute = 60.0;
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
    let clone_translator_v2 = translator_v2.clone();
    tokio::spawn(async move {
        clone_translator_v2.start().await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    (translator_v2, listening_address)
}

pub fn measure_hashrate(duration_secs: u64) -> f64 {
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

pub async fn start_mining_sv2_proxy(upstream: SocketAddr) -> SocketAddr {
    use mining_proxy_sv2::{ChannelKind, UpstreamMiningValues};
    let upstreams = vec![UpstreamMiningValues {
        address: upstream.ip().to_string(),
        port: upstream.port(),
        pub_key: Secp256k1PublicKey::from_str(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72",
        )
        .unwrap(),
        channel_kind: ChannelKind::Extended,
    }];
    let mining_proxy_listening_address = get_available_address();
    let config = mining_proxy_sv2::Configuration {
        upstreams,
        listen_address: mining_proxy_listening_address.ip().to_string(),
        listen_mining_port: mining_proxy_listening_address.port(),
        max_supported_version: 2,
        min_supported_version: 2,
        downstream_share_per_minute: 1.0,
        expected_total_downstream_hr: 10_000.0,
        reconnect: true,
    };
    tokio::spawn(async move {
        mining_proxy_sv2::start_mining_proxy(config).await;
    });
    mining_proxy_listening_address
}
