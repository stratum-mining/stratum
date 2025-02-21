// This file contains integration tests for the `TranslatorSv2` module.
//
// `TranslatorSv2` is a module that implements the Translator role in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use const_sv2::{
    MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH, MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
};
use integration_tests_sv2::{sniffer::*, *};
use roles_logic_sv2::{
    mining_sv2::OpenExtendedMiningChannelSuccess,
    parsers::{CommonMessages, Mining, PoolMessages},
};

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[ignore]
#[tokio::test]
async fn translate_sv1_to_sv2_successfully() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (pool_translator_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None).await;
    let (_, tproxy_addr) = start_sv2_translator(pool_translator_sniffer_addr).await;
    let _mining_device = start_mining_device_sv1(tproxy_addr, false, None).await;
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
}

// Test full flow with Translator and Job Declarator. An SV1 mining device is connected to
// Translator and is successfully submitting shares to the pool. It also asserts that Pool role
// sends a subsequent `SET_NEW_PREV_HASH`.
#[tokio::test]
async fn translation_proxy_and_jd() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (jdc_pool_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (_jdc, jdc_addr) = start_jdc(pool_translator_sniffer_addr, tp_addr, jds_addr).await;
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr).await;
    jdc_pool_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    assert_common_message!(
        &jdc_pool_sniffer.next_message_from_downstream(),
        SetupConnection
    );
    assert_common_message!(
        &jdc_pool_sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    assert_mining_message!(
        &jdc_pool_sniffer.next_message_from_downstream(),
        OpenExtendedMiningChannel
    );
    let open_extended_mining_channel_success = jdc_pool_sniffer
        .includes_message_type_with_message(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
        )
        .await;
    let open_extended_mining_channel_success = match open_extended_mining_channel_success {
        Some(PoolMessages::Mining(Mining::OpenExtendedMiningChannelSuccess(msg))) => msg,
        _ => panic!("Unexpected message type"),
    };
    let mut target = [0; 32];
    target.copy_from_slice(open_extended_mining_channel_success.target.inner_as_ref());
    let _mining_device = start_mining_device_sv1(tproxy_addr, true, None).await;
    assert_mining_message!(
        &jdc_pool_sniffer.next_message_from_upstream(),
        OpenExtendedMiningChannelSuccess
    );
    assert_mining_message!(
        &jdc_pool_sniffer.next_message_from_upstream(),
        NewExtendedMiningJob
    );
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        )
        .await;
}
