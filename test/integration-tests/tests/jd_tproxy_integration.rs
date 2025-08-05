use integration_tests_sv2::{interceptor::MessageDirection, template_provider::DifficultyLevel, *};
use stratum_common::roles_logic_sv2::{common_messages_sv2::*, mining_sv2::*};

#[tokio::test]
async fn jd_tproxy_integration() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (jdc_pool_sniffer, jdc_pool_sniffer_addr) =
        start_sniffer("0", pool_addr, false, vec![], None);
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (_jdc, jdc_addr) = start_jdc(&[(jdc_pool_sniffer_addr, jds_addr)], tp_addr);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    start_mining_device_sv1(tproxy_addr, false, None);
    jdc_pool_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        )
        .await;
}
