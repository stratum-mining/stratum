use integration_tests_sv2::{
    interceptor::{MessageDirection, ReplaceMessage},
    *,
};
use std::convert::TryInto;
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::*,
    mining_sv2::{SubmitSharesError, *},
    parsers::{AnyMessage, Mining},
};

// Tests whether JDC will switch to a new pool after receiving a `SubmitSharesError` message from
// the currently connected pool.
//
// This ignore directive can be removed once this issue is resolved: https://github.com/stratum-mining/stratum/issues/1574.
#[ignore]
#[tokio::test]
async fn test_jdc_pool_fallback_after_submit_rejection() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool_1, pool_addr_1) = start_pool(Some(tp_addr)).await;
    // Sniffer between JDC and first pool
    let (sniffer_1, sniffer_addr_1) = start_sniffer(
        "0",
        pool_addr_1,
        false,
        vec![
            // Should trigger Fallback process in JDC
            ReplaceMessage::new(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
                AnyMessage::Mining(Mining::SubmitSharesError(SubmitSharesError {
                    channel_id: 0,
                    sequence_number: 0,
                    error_code: "invalid-nonce".to_string().into_bytes().try_into().unwrap(),
                })),
            )
            .into(),
        ],
    );
    let (_pool_2, pool_addr_2) = start_pool(Some(tp_addr)).await;
    // Sniffer between JDC and second pool
    let (sniffer_2, sniffer_addr_2) = start_sniffer("1", pool_addr_2, false, vec![]);
    let (_jds_1, jds_addr_1) = start_jds(tp.rpc_info());
    // Sniffer between JDC and first JDS
    let (sniffer_3, sniffer_addr_3) = start_sniffer("2", jds_addr_1, false, vec![]);
    let (_jds_2, jds_addr_2) = start_jds(tp.rpc_info());
    // Sniffer between JDC and second JDS
    let (sniffer_4, sniffer_addr_4) = start_sniffer("3", jds_addr_2, false, vec![]);
    let (_jdc, jdc_addr) = start_jdc(
        &[
            (sniffer_addr_1, sniffer_addr_3),
            (sniffer_addr_2, sniffer_addr_4),
        ],
        tp_addr,
    );
    // Assert that JDC has connected to the first (Pool,JDS) pair
    sniffer_1
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    sniffer_3
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    let (_translator, sv2_translator_addr) = start_sv2_translator(jdc_addr);
    start_mining_device_sv1(sv2_translator_addr, false, None);
    // Assert that JDC switched to the second (Pool,JDS) pair
    sniffer_2
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    sniffer_4
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
}
