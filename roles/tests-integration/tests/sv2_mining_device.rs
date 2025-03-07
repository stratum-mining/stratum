use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS};
use integration_tests_sv2::*;
use sniffer::MessageDirection;

#[tokio::test]
async fn sv2_mining_device_and_pool_success() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer, sniffer_addr) = start_sniffer("A".to_string(), pool_addr, false, None).await;
    let _sv2_mining_device =
        start_mining_device_sv2(sniffer_addr, None, None, None, 1, None, true).await;
    sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
}
