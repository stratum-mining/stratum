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

#[tokio::test]
async fn test_sniffer_interrupter() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let message =
        PoolMessages::Common(CommonMessages::SetupConnectionError(SetupConnectionError {
            flags: 0,
            error_code: "unsupported-feature-flags"
                .to_string()
                .into_bytes()
                .try_into()
                .unwrap(),
        }));
    let interrupt_msgs = InterceptMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        message,
        MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
        true,
    );
    let (sniffer, sniffer_addr) =
        start_sniffer("".to_string(), tp_addr, false, Some(vec![interrupt_msgs])).await;
    let _ = start_pool(Some(sniffer_addr)).await;
    assert_common_message!(&sniffer.next_message_from_downstream(), SetupConnection);
    assert_common_message!(&sniffer.next_message_from_upstream(), SetupConnectionError);
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
