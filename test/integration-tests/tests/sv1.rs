#![cfg(feature = "sv1")]
use integration_tests_sv2::*;

#[tokio::test]
async fn test_basic_sv1() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_, tproxy_addr) = start_sv2_translator(pool_addr).await;
    let (sniffer_sv1, sniffer_sv1_addr) = start_sv1_sniffer(tproxy_addr);
    let _mining_device = start_mining_device_sv1(sniffer_sv1_addr, false, None).await;
    sniffer_sv1
        .wait_for_message_from_downstream("mining.configure")
        .await;
    sniffer_sv1
        .wait_for_message_from_upstream(&[
            "minimum-difficulty",
            "version-rolling",
            "version-rolling.mask",
            "version-rolling.min-bit-count",
        ])
        .await;
}
