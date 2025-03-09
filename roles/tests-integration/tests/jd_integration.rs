use std::time::Duration;

// This file contains integration tests for the `JDC/S` module.
//
// `JDC/S` are modules that implements the Job Decleration roles in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use const_sv2::{
    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
};
use integration_tests_sv2::{sniffer::MessageDirection, *};
use roles_logic_sv2::parsers::AnyMessage;

use roles_logic_sv2::parsers::CommonMessages;

// This test verifies that jd-server does not exit when a connected jd-client shuts down.
//
// It is performing the verification by shutding down a jd-client connected to a jd-server and then
// starting a new jd-client that connects to the same jd-server successfully.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (jdc, jdc_addr) = start_jdc(pool_addr, tp_addr, jds_addr).await;
    jdc.shutdown();
    // wait for shutdown to complete
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc_1, _jdc_addr_1) = start_jdc(pool_addr, tp_addr, sniffer_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}

// This test verifies that jd-client exchange SetupConnection messages with a Template Provider.
//
// Note that jd-client starts to exchange messages with the Template Provider after it has accepted
// a downstream connection.
#[tokio::test]
async fn jdc_tp_success_setup() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (tp_jdc_sniffer, tp_jdc_sniffer_addr) =
        start_sniffer("0".to_string(), tp_addr, false, None).await;
    let (_jdc, jdc_addr) = start_jdc(pool_addr, tp_jdc_sniffer_addr, jds_addr).await;
    // This is needed because jd-client waits for a downstream connection before it starts
    // exchanging messages with the Template Provider.
    start_sv2_translator(jdc_addr).await;
    tp_jdc_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    tp_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
}

/// This test ensures that `jd-client` does not panic even if `jd-server` leaves the connection open
/// after receiving the request for token.
///
/// The test verifies whether `jdc` has crashed by attempting to bind to the `jdc` port after 10
/// seconds of no response from `jd-server`.
#[tokio::test]
async fn jdc_does_not_stackoverflow_when_no_token() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let block_from_message = sniffer::IgnoreFromMessage::new(
        sniffer::MessageDirection::ToDownstream,
        MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    );
    let (jds_jdc_sniffer, jds_jdc_sniffer_addr) = start_sniffer(
        "JDS-JDC-sniffer".to_string(),
        jds_addr,
        false,
        Some(block_from_message.into()),
    )
    .await;
    let (_jdc, jdc_addr) = start_jdc(pool_addr, tp_addr, jds_jdc_sniffer_addr).await;
    let (_, _) = start_sv2_translator(jdc_addr).await;
    jds_jdc_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    jds_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;

    jds_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        )
        .await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_err());
}
