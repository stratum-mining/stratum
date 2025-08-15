use integration_tests_sv2::{interceptor::MessageDirection, template_provider::DifficultyLevel, *};
use stratum_common::roles_logic_sv2::job_declaration_sv2::*;

#[tokio::test]
async fn jds_ask_for_missing_transactions() {
    start_tracing();
    let (tp_1, tp_addr_1) = start_template_provider(None, DifficultyLevel::Low);
    let (tp_2, tp_addr_2) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr_1)).await;
    let (_jds, jds_addr) = start_jds(tp_1.rpc_info());
    let (sniffer, sniffer_addr) = start_sniffer("A", jds_addr, false, vec![], None);
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, sniffer_addr)], tp_addr_2);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    start_mining_device_sv1(tproxy_addr, false, None);
    assert!(tp_2.fund_wallet().is_ok());
    assert!(tp_2.create_mempool_transaction().is_ok());
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_DECLARE_MINING_JOB,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
        )
        .await;
}
