use const_sv2::{
    MESSAGE_TYPE_SETUP_CONNECTION_ERROR, MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    MESSAGE_TYPE_SET_NEW_PREV_HASH,
};
use integration_tests_sv2::*;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionError,
    parsers::{CommonMessages, PoolMessages},
};
use sniffer::{InterceptMessage, MessageDirection};
use std::convert::TryInto;

// this test aims to assert that Sniffer is able to intercept and replace some message
// sniffer_a replaces a SetupConnectionSuccess from TP with a SetupConnectionError directed at Pool
// sniffer_b asserts that Pool is about to receive a SetupConnectionError
// TP -> sniffer_a -> sniffer_b -> Pool
#[tokio::test]
async fn test_sniffer_intercept() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let message_replacement =
        PoolMessages::Common(CommonMessages::SetupConnectionError(SetupConnectionError {
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
        MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
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
async fn test_sniffer_wait_for_message_type_with_remove() {
    let (_tp, tp_addr) = start_template_provider(None).await;
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
