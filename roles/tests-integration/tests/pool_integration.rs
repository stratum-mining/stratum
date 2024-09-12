use std::str::FromStr;

use common::is_port_open;

mod common;

#[tokio::test]
async fn success_pool_template_provider_connection() {
    assert!(common::start_template_provider_and_pool().await.is_ok());
}

#[tokio::test]
async fn pool_bad_coinbase_output() {
    let (template_provider, template_provider_port) = common::start_template_provider().await;
    let invalid_coinbase_output = vec![pool_sv2::mining_pool::CoinbaseOutput::new(
	"P2PK".to_string(),
	"04466d7fcae563e5cb09a0d1870bb580344804617879a14949cf22285f1bae3f276728176c3c6431f8eeda4538dc37c865e2784f3a9e77d044f33e407797e1278".to_string(),
  )];
    let template_provider_address =
        std::net::SocketAddr::from_str(&format!("127.0.0.1:{}", template_provider_port)).unwrap();
    let test_pool = common::TestPoolSv2::new(
        None,
        Some(invalid_coinbase_output),
        Some(template_provider_address),
    );
    let pool = test_pool.pool.clone();
    let state = pool.state().await.safe_lock(|s| s.clone()).unwrap();
    assert_eq!(state, pool_sv2::PoolState::Initial);
    assert!(pool.start().await.is_err());
    let state = pool.state().await.safe_lock(|s| s.clone()).unwrap();
    assert_eq!(state, pool_sv2::PoolState::Initial);
    template_provider.stop();
}

#[cfg(feature = "mining_device_reject_auth")]
#[tokio::test]
async fn pool_remove_downstream_after_bad_auth() {
    let (template_provider, template_provider_port) = common::start_template_provider().await;
    let test_pool = common::TestPoolSv2::new(
        None,
        None,
        Some(
            std::net::SocketAddr::from_str(&format!("127.0.0.1:{}", template_provider_port))
                .unwrap(),
        ),
    );
    let pool = test_pool.pool.clone();
    let state = pool.state().await.safe_lock(|s| s.clone()).unwrap();
    assert_eq!(state, pool_sv2::PoolState::Initial);
    let _pool = pool.clone();
    tokio::task::spawn(async move {
        let _ = _pool.start().await;
    });
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    // Wait for the pool to start.
    let pool_port = test_pool.port;
    let pool_listening_address =
        std::net::SocketAddr::from_str(&format!("127.0.0.1:{}", pool_port)).unwrap();
    loop {
        if is_port_open(pool_listening_address) {
            break;
        }
    }
    let state = pool.state().await.safe_lock(|s| s.clone()).unwrap();
    assert_eq!(
        state,
        pool_sv2::PoolState::Running(pool_sv2::DroppedDownstreams::new())
    );
    assert!(common::TestMiningDevice::start(pool_listening_address)
        .await
        .is_err());
    let state = pool.state().await.safe_lock(|s| s.clone()).unwrap();
    let mut dropped_downstreams = pool_sv2::DroppedDownstreams::new();
    dropped_downstreams.push(1);
    assert_eq!(state, pool_sv2::PoolState::Running(dropped_downstreams));
    template_provider.stop();
}
