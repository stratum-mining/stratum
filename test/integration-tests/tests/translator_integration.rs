// This file contains integration tests for the `TranslatorSv2` module.
use integration_tests_sv2::{interceptor::MessageDirection, *};
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::*,
    mining_sv2::*,
    parsers::{AnyMessage, CommonMessages, Mining},
};
use tokio_util::sync::CancellationToken;

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[tokio::test]
async fn translate_sv1_to_sv2_successfully() {
    start_tracing();
    let mining_device_cancel_token = CancellationToken::new();
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (pool_translator_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0", pool_addr, false, vec![]);
    let (_, tproxy_addr) = start_sv2_translator(pool_translator_sniffer_addr);
    start_mining_device_sv1(tproxy_addr, false, None, mining_device_cancel_token.clone());
    pool_translator_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    assert_common_message!(
        &pool_translator_sniffer.next_message_from_downstream(),
        SetupConnection
    );
    assert_common_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_downstream(),
        OpenExtendedMiningChannel
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        OpenExtendedMiningChannelSuccess
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        NewExtendedMiningJob
    );
    pool_translator_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        )
        .await;

    mining_device_cancel_token.cancel();
}
