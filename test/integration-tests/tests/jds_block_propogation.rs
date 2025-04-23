use integration_tests_sv2::{sniffer::*, *};
use stratum_common::{MESSAGE_TYPE_PUSH_SOLUTION, MESSAGE_TYPE_SUBMIT_SOLUTION};

// Block propogated from JDS to TP
#[tokio::test]
async fn propogated_from_jds_to_tp() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let current_block_hash = tp.get_best_block_hash().unwrap();
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (jdc_jds_sniffer, jdc_jds_sniffer_addr) =
        start_sniffer("0".to_string(), jds_addr, false, None);
    let ignore_submit_solution =
        IgnoreMessage::new(MessageDirection::ToUpstream, MESSAGE_TYPE_SUBMIT_SOLUTION);
    let (jdc_tp_sniffer, jdc_tp_sniffer_addr) = start_sniffer(
        "1".to_string(),
        tp_addr,
        false,
        Some(ignore_submit_solution.into()),
    );
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, jdc_jds_sniffer_addr)], jdc_tp_sniffer_addr);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    let _mining_device = start_mining_device_sv1(tproxy_addr, false, None);
    jdc_jds_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_PUSH_SOLUTION)
        .await;
    jdc_tp_sniffer
        .assert_message_not_present(MessageDirection::ToUpstream, MESSAGE_TYPE_SUBMIT_SOLUTION)
        .await;
    let new_block_hash = tp.get_best_block_hash().unwrap();
    assert_ne!(current_block_hash, new_block_hash);
}
