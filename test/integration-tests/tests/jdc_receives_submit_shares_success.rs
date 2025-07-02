use integration_tests_sv2::{interceptor::MessageDirection, *};
use stratum_common::roles_logic_sv2::mining_sv2::*;

#[tokio::test]
async fn jdc_submit_shares_success() {
    start_tracing();
    let mining_device_cancel_token = tokio_util::sync::CancellationToken::new();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer, sniffer_addr) = start_sniffer("0", pool_addr, false, vec![]);
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (_jdc, jdc_addr) = start_jdc(&[(sniffer_addr, jds_addr)], tp_addr);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    start_mining_device_sv1(tproxy_addr, false, None, mining_device_cancel_token.clone());

    // make sure sure JDC gets a share acknowledgement
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        )
        .await;

    mining_device_cancel_token.cancel();
}
