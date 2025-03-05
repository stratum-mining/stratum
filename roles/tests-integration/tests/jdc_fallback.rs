use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS};
use integration_tests_sv2::*;
use roles_logic_sv2::{
    mining_sv2::SubmitSharesError,
    parsers::{AnyMessage, CommonMessages, Mining},
};
use sniffer::{InterceptMessage, MessageDirection};
use std::convert::TryInto;

// Start JDC with two pools in the pool list config. The test forces the first pool to send a share
// submission rejection by altering a message(MINING_SET_NEW_PREV_HASH) sent by the pool. The test
// verifies that the second pool will be used as a fallback.
#[tokio::test]
async fn test_jdc_pool_fallback_after_submit_rejection() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer_1, sniffer_addr) = start_sniffer(
        "0".to_string(),
        pool_addr,
        false,
        Some(vec![InterceptMessage::new(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            AnyMessage::Mining(Mining::SubmitSharesError(SubmitSharesError {
                channel_id: 0,
                sequence_number: 0,
                error_code: "invalid-nonce".to_string().into_bytes().try_into().unwrap(),
            })),
        )]),
    )
    .await;
    let (_pool_2, pool_addr_2) = start_pool(Some(tp_addr)).await;
    let (sniffer_2, sniffer_addr_2) =
        start_sniffer("1".to_string(), pool_addr_2, false, None).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (_jdc, jdc_addr) = start_jdc(vec![sniffer_addr, sniffer_addr_2], tp_addr, jds_addr).await;
    assert_common_message!(&sniffer_1.next_message_from_downstream(), SetupConnection);
    let (_translator, sv2_translator_addr) = start_sv2_translator(jdc_addr).await;
    let _ = start_mining_device_sv1(sv2_translator_addr, true, None).await;
    sniffer_2
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
}
