// This file contains integration tests for the `JDC/S` module.
//
// `JDC/S` are modules that implements the Job Decleration roles in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS};
use integration_tests_sv2::*;
use roles_logic_sv2::{
    mining_sv2::SubmitSharesError,
    parsers::{CommonMessages, Mining, PoolMessages},
};
use sniffer::{InterceptMessage, MessageDirection};
use std::convert::TryInto;

// This test verifies that the `jds` (Job Decleration Server) does not panic when the `jdc`
// (Job Decleration Client) shuts down.
//
// The test follows these steps:
// 1. Start a Template Provider (`tp`) and a Pool.
// 2. Start the `jds` and the `jdc`, ensuring the `jdc` connects to the `jds`.
// 3. Shut down the `jdc` and ensure the `jds` remains operational without panicking.
// 4. Verify that the `jdc`'s address can be reused by asserting that a new `TcpListener` can bind
//    to the same address.
// 5. Start a Sniffer as a proxy for observing messages exchanged between the `jds` and other
//    components.
// 6. Reconnect a new `jdc` to the Sniffer and ensure the expected `SetupConnection` message is
//    received from the `jdc`.
//
// This ensures that the shutdown of the `jdc` is handled gracefully by the `jds`, and subsequent
// connections or operations continue without issues.
//
// # Notes
// - The test waits for a brief duration (`1 second`) after shutting down the `jdc` to allow for any
//   internal cleanup or reconnection attempts.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp_addr).await;
    let (jdc, jdc_addr) = start_jdc(vec![pool_addr], tp_addr, jds_addr).await;
    jdc.shutdown();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc, _jdc_addr) = start_jdc(vec![pool_addr], tp_addr, sniffer_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}

// Start JDClient with two pools in the pool list config. The test forces the first pool to send a
// share submission rejection by altering a message(MINING_SET_NEW_PREV_HASH) sent by the pool. The
// test verifies that the second pool will be used as a fallback.
#[tokio::test]
async fn test_jdc_pool_fallback_after_submit_rejection() {
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer_1, sniffer_addr) = start_sniffer(
        "0".to_string(),
        pool_addr,
        false,
        Some(vec![InterceptMessage::new(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            PoolMessages::Mining(Mining::SubmitSharesError(SubmitSharesError {
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
    let (_jds, jds_addr) = start_jds(tp_addr).await;
    let (_jdc, jdc_addr) = start_jdc(vec![sniffer_addr, sniffer_addr_2], tp_addr, jds_addr).await;
    assert_common_message!(&sniffer_1.next_message_from_downstream(), SetupConnection);
    let (_translator, sv2_translator_addr) = start_sv2_translator(jdc_addr).await;
    let _ = start_mining_device_sv1(sv2_translator_addr, true).await;
    sniffer_2
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
}
