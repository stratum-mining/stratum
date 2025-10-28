use integration_tests_sv2::{interceptor::MessageDirection, template_provider::DifficultyLevel, *};
use stratum_apps::stratum_core::mining_sv2::*;

#[tokio::test]
async fn jdc_submit_shares_success() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer, sniffer_addr) = start_sniffer("0", pool_addr, false, vec![], None);
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (_jdc, jdc_addr) = start_jdc(&[(sniffer_addr, jds_addr)], tp_addr);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr, false).await;
    let (_minerd_process, _minerd_addr) = start_minerd(tproxy_addr, None, None, false).await;

    // make sure sure JDC gets a share acknowledgement
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        )
        .await;
}
