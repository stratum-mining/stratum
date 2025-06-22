// This file contains integration tests for the `Sniffer` module.
use integration_tests_sv2::{
    interceptor::{IgnoreMessage, MessageDirection, ReplaceMessage},
    template_provider::DifficultyLevel,
    *,
};
use std::convert::TryInto;
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionSuccess, *},
    parsers_sv2::{AnyMessage, CommonMessages},
    template_distribution_sv2::*,
};

// This test aims to assert that Sniffer is able to intercept and replace/ignore messages.
// TP -> sniffer_a -> sniffer_b -> Pool
#[tokio::test]
async fn test_sniffer_interception() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let ignore_message =
        IgnoreMessage::new(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE);
    let setup_connection_message =
        AnyMessage::Common(CommonMessages::SetupConnection(SetupConnection {
            protocol: Protocol::TemplateDistributionProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: b"0.0.0.0".to_vec().try_into().unwrap(),
            endpoint_port: 8081,
            vendor: b"Bitmain".to_vec().try_into().unwrap(),
            hardware_version: b"901".to_vec().try_into().unwrap(),
            firmware: b"abcX".to_vec().try_into().unwrap(),
            device_id: b"89567".to_vec().try_into().unwrap(),
        }));
    let setup_connection_replacement = ReplaceMessage::new(
        MessageDirection::ToUpstream,
        MESSAGE_TYPE_SETUP_CONNECTION,
        setup_connection_message,
    );
    let setup_connection_error_message = AnyMessage::Common(
        CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
            flags: 0,
            used_version: 0,
        }),
    );
    let setup_connection_success_replacement = ReplaceMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        setup_connection_error_message,
    );
    let (sniffer_a, sniffer_a_addr) = start_sniffer(
        "A",
        tp_addr,
        false,
        vec![
            setup_connection_success_replacement.into(),
            ignore_message.into(),
        ],
        None,
    );
    let (sniffer_b, sniffer_b_addr) = start_sniffer(
        "B",
        sniffer_a_addr,
        false,
        vec![setup_connection_replacement.into()],
        None,
    );
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
    sniffer_b
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    assert_common_message!(
        &sniffer_b.next_message_from_upstream(),
        SetupConnectionSuccess,
        used_version,
        0,
        flags,
        0
    );
    assert!(
        !(sniffer_b
            .includes_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE))
    );
}

#[tokio::test]
async fn test_sniffer_wait_for_message_type_with_remove() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (sniffer, sniffer_addr) = start_sniffer("", tp_addr, false, vec![], None);
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
