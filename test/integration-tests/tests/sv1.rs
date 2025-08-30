#![cfg(feature = "sv1")]
use integration_tests_sv2::{template_provider::DifficultyLevel, *};
use interceptor::MessageDirection;

#[tokio::test]
async fn test_basic_sv1() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_, tproxy_addr) = start_sv2_translator(pool_addr).await;
    let (sniffer_sv1, sniffer_sv1_addr) = start_sv1_sniffer(tproxy_addr);
    let (_minerd_process, _minerd_addr) = start_minerd(sniffer_sv1_addr, None, None, false).await;
    sniffer_sv1
        .wait_for_message(&["mining.subscribe"], MessageDirection::ToUpstream)
        .await;
    sniffer_sv1
        .wait_for_message(&["mining.authorize"], MessageDirection::ToUpstream)
        .await;
    sniffer_sv1
        .wait_for_message(&["mining.set_difficulty"], MessageDirection::ToDownstream)
        .await;
    sniffer_sv1
        .wait_for_message(&["mining.notify"], MessageDirection::ToDownstream)
        .await;
}
