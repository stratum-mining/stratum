// This file contains integration tests for the `Sniffer` module.
//
// `Sniffer` is a useful tool to perform Man-in-the-Middle setups for testing purposes.  It can
// intercept messages and replace them with others, as well as assert that certain messages were
// received.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use const_sv2::{
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    MESSAGE_TYPE_SET_NEW_PREV_HASH,
};
use integration_tests_sv2::*;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionError},
    parsers::{AnyMessage, CommonMessages},
};
use sniffer::{InterceptMessage, MessageDirection};
use std::convert::TryInto;

// This test aims to assert that Sniffer is able to intercept and replace some messages.
// sniffer_a replaces a SetupConnectionSuccess from TP with a SetupConnectionError directed at Pool
// sniffer_b asserts that Pool is about to receive a SetupConnectionError
// TP -> sniffer_a -> sniffer_b -> Pool
#[tokio::test]
async fn test_sniffer_intercept_to_downstream() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let message_replacement =
        AnyMessage::Common(CommonMessages::SetupConnectionError(SetupConnectionError {
            flags: 0,
            error_code: "unsupported-feature-flags"
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap(),
        }));
    let intercept = InterceptMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        message_replacement,
    );

    // this sniffer will replace SetupConnectionSuccess with SetupConnectionError
    let (_sniffer_a, sniffer_a_addr) =
        start_sniffer("A".to_string(), tp_addr, false, Some(vec![intercept])).await;

    // this sniffer will assert SetupConnectionSuccess was correctly replaced with
    // SetupConnectionError
    let (sniffer_b, sniffer_b_addr) =
        start_sniffer("B".to_string(), sniffer_a_addr, false, None).await;

    let _ = start_pool(Some(sniffer_b_addr)).await;

    // assert sniffer_a functionality of replacing messages work as expected (goal of this test)
    assert_common_message!(
        &sniffer_b.next_message_from_upstream(),
        SetupConnectionError
    );
}

#[tokio::test]
async fn test_sniffer_intercept_to_upstream() {
    let (_tp, tp_addr) = start_template_provider(None);
    let setup_connection = SetupConnection {
        protocol: Protocol::TemplateDistributionProtocol,
        min_version: 2,
        max_version: 2,
        flags: 0,
        endpoint_host: "0.0.0.0".to_string().into_bytes().try_into().unwrap(),
        endpoint_port: 8081,
        vendor: "Bitmain".to_string().into_bytes().try_into().unwrap(),
        hardware_version: "901".to_string().into_bytes().try_into().unwrap(),
        firmware: "abcX".to_string().into_bytes().try_into().unwrap(),
        device_id: "89567".to_string().into_bytes().try_into().unwrap(),
    };
    let message_replacement = AnyMessage::Common(CommonMessages::SetupConnection(setup_connection));
    let intercept = InterceptMessage::new(
        MessageDirection::ToUpstream,
        MESSAGE_TYPE_SETUP_CONNECTION,
        message_replacement,
    );

    let (sniffer_a, sniffer_a_addr) =
        start_sniffer("A".to_string(), tp_addr, false, Some(vec![intercept])).await;

    let (_sniffer_b, sniffer_b_addr) =
        start_sniffer("B".to_string(), sniffer_a_addr, false, None).await;

    let _ = start_pool(Some(sniffer_b_addr)).await;

    assert_common_message!(
        &sniffer_a.next_message_from_downstream(),
        SetupConnection,
        protocol,
        Protocol::TemplateDistributionProtocol,
        flags,
        0,
        min_version,
        2,
        max_version,
        2,
        endpoint_host,
        "0.0.0.0".to_string().into_bytes().try_into().unwrap(),
        endpoint_port,
        8081,
        vendor,
        "Bitmain".to_string().into_bytes().try_into().unwrap()
    );
}

#[tokio::test]
async fn test_sniffer_wait_for_message_type_with_remove() {
    let (_tp, tp_addr) = start_template_provider(None);
    let (sniffer, sniffer_addr) = start_sniffer("".to_string(), tp_addr, false, None).await;
    let _ = start_pool(Some(sniffer_addr)).await;
    assert!(
        sniffer
            .wait_for_message_type_and_clean_queue(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )
            .await
    );
    assert_eq!(
        sniffer
            .includes_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS
            )
            .await,
        false
    );
    assert_eq!(
        sniffer
            .includes_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SET_NEW_PREV_HASH
            )
            .await,
        false
    );
}
