// This file contains integration tests for the `Sniffer` module.
use const_sv2::{
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, MESSAGE_TYPE_SET_NEW_PREV_HASH,
};
use integration_tests_sv2::{sniffer::IgnoreMessage, *};
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionError},
    parsers::{AnyMessage, CommonMessages},
};
use sniffer::{MessageDirection, ReplaceMessage};
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
    let intercept = ReplaceMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        message_replacement,
    );
    // this sniffer will replace SetupConnectionSuccess with SetupConnectionError
    let (_sniffer_a, sniffer_a_addr) =
        start_sniffer("A".to_string(), tp_addr, false, Some(intercept.into()));
    // this sniffer will assert SetupConnectionSuccess was correctly replaced with
    // SetupConnectionError
    let (sniffer_b, sniffer_b_addr) = start_sniffer("B".to_string(), sniffer_a_addr, false, None);
    let _ = start_pool(Some(sniffer_b_addr)).await;
    // assert sniffer_a functionality of replacing messages work as expected (goal of this test)
    sniffer_b
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
        )
        .await;
}

#[tokio::test]
async fn test_sniffer_intercept_to_upstream() {
    start_tracing();
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
    let intercept = ReplaceMessage::new(
        MessageDirection::ToUpstream,
        MESSAGE_TYPE_SETUP_CONNECTION,
        message_replacement,
    );
    let (sniffer_a, sniffer_a_addr) =
        start_sniffer("A".to_string(), tp_addr, false, Some(intercept.into()));
    let (_sniffer_b, sniffer_b_addr) = start_sniffer("B".to_string(), sniffer_a_addr, false, None);
    let _ = start_pool(Some(sniffer_b_addr)).await;
    sniffer_a
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
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
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let (sniffer, sniffer_addr) = start_sniffer("".to_string(), tp_addr, false, None);
    let _ = start_pool(Some(sniffer_addr)).await;
    assert!(
        sniffer
            .wait_for_message_type_and_clean_queue(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )
            .await
    );
    assert!(
        !(sniffer.includes_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS
        ))
    );
    assert!(
        !(sniffer.includes_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SET_NEW_PREV_HASH
        ))
    );
}

// Verifies that [`Sniffer`] can intercept and block a message stream.
//
// This test sets up a chain where a message from the Template Provider (TP)
// passes through three sniffers (`sniffer_a`, `sniffer_b` and `sniffer_c`) before reaching the
// Pool.
//
// - `sniffer_a` is configured to intercept `SetupConnectionSuccess` messages directed downstream.
// - `sniffer_b` is configured to block `SetupConnectionSuccess` messages directed downstream.
// - `sniffer_c` should receive no messages after initial setup, ensuring the block works.
//
// **Flow:**
// `TP -> sniffer_a -> sniffer_b -> sniffer_c -> Pool`
#[tokio::test]
async fn test_sniffer_blocks_message() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    // Define an action to ignore SetupConnectionSuccess messages going downstream.
    let ignore_message = IgnoreMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    );
    // `sniffer_a` intercepts and receives `SetupConnectionSuccess` message.
    let (sniffer_a, sniffer_a_addr) = start_sniffer("A".to_string(), tp_addr, false, None);
    // `sniffer_b` is placed downstream of `sniffer_a` and ignores `SetupConnectionSuccess` message.
    let (_sniffer_b, sniffer_b_addr) = start_sniffer(
        "B".to_string(),
        sniffer_a_addr,
        false,
        Some(ignore_message.into()),
    );
    // `sniffer_c` is placed downstream of `sniffer_b` and should not receive the ignored message.
    let (sniffer_c, sniffer_c_addr) = start_sniffer("C".to_string(), sniffer_b_addr, false, None);
    // Start the Pool, connected to `sniffer_c`.
    let _ = start_pool(Some(sniffer_c_addr)).await;
    // Block waiting for intercepting setup connection success on sniffer_a
    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    // Assert that `sniffer_c` does not receive any messages, confirming `sniffer_b`'s block works.
    assert!(sniffer_c.next_message_from_upstream().is_none());
}
