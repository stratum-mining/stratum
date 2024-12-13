mod common;

use std::convert::TryInto;

use common::sniffer::{InterceptMessage, MessageDirection};
use const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnectionError,
    parsers::{CommonMessages, PoolMessages},
};

#[tokio::test]
async fn test_sniffer_interrupter() {
    let (_tp, tp_addr) = common::start_template_provider().await;
    use const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS;
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
        common::start_sniffer("".to_string(), tp_addr, false, Some(vec![interrupt_msgs])).await;
    let _ = common::start_pool(Some(sniffer_addr)).await;
    assert_common_message!(&sniffer.next_message_from_downstream(), SetupConnection);
    assert_common_message!(&sniffer.next_message_from_upstream(), SetupConnectionError);
}
