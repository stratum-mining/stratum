use integration_tests_sv2::{sniffer::MessageDirection, *};
use stratum_common::{
    MESSAGE_TYPE_DECLARE_MINING_JOB, MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
    MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
};

#[tokio::test]
async fn jds_ask_for_missing_transactions() {
    start_tracing();
    let (tp_1, tp_addr_1) = start_template_provider(None);
    let (tp_2, tp_addr_2) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr_1)).await;
    let (_jds, jds_addr) = start_jds(tp_1.rpc_info());
    let (sniffer, sniffer_addr) = start_sniffer("A".to_string(), jds_addr, false, vec![]);
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, sniffer_addr)], tp_addr_2);
    start_sv2_translator(jdc_addr);
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
