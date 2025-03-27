// This file contains integration tests for the `TranslatorSv2` module.
use const_sv2::{
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
    MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
};
use integration_tests_sv2::{sniffer::*, *};
use roles_logic_sv2::parsers::{AnyMessage, CommonMessages, Mining};

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[tokio::test]
async fn translate_sv1_to_sv2_successfully() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (pool_translator_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None);
    let (_, tproxy_addr) = start_sv2_translator(pool_translator_sniffer_addr);
    let _mining_device = start_mining_device_sv1(tproxy_addr, false, None);
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
// Translator and is successfully submitting a share to the pool.
//
// This ignore directive can be removed once this issue is resolved: https://github.com/stratum-mining/stratum/issues/1574.
#[ignore]
#[tokio::test]
async fn translation_proxy_and_jd() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (jdc_pool_sniffer, jdc_pool_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None);
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (_jdc, jdc_addr) = start_jdc(&[(jdc_pool_sniffer_addr, jds_addr)], tp_addr);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    let _mining_device = start_mining_device_sv1(tproxy_addr, true, None);
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
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        )
        .await;
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        )
        .await;
}
