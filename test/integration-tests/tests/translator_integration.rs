// This file contains integration tests for the `TranslatorSv2` module.
//
// `TranslatorSv2` is a module that implements the Translator role in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use const_sv2::{
    MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
    MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS, MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES
};
use integration_tests_sv2::{sniffer::*, *};
use roles_logic_sv2::parsers::{
    AnyMessage, CommonMessages, Mining,
};
use roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess;
use std::convert::TryInto;

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
// Translator and is successfully submitting a share to the pool.
#[tokio::test]
async fn translation_proxy_and_jd() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (jdc_pool_sniffer, jdc_pool_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (_jdc, jdc_addr) = start_jdc(&[(jdc_pool_sniffer_addr, jds_addr)], tp_addr).await;
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr).await;
    let _mining_device = start_mining_device_sv1(tproxy_addr, true, None).await;
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

// when a tProxy receives a OpenExtendedMiningChannelSuccess with a bad extranonce_size
// we expect it to shut down gracefully
#[tokio::test]
async fn tproxy_refuses_bad_extranonce_size() {
    let (_tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;

    let message_replacement = AnyMessage::Mining(Mining::OpenExtendedMiningChannelSuccess(
        OpenExtendedMiningChannelSuccess {
            request_id: 0,
            channel_id: 1,
            target: [
                112, 123, 89, 188, 221, 164, 162, 167, 139, 39, 104, 137, 2, 111, 185, 17, 165, 85,
                33, 115, 67, 45, 129, 197, 134, 103, 128, 151, 59, 19, 0, 0,
            ]
            .into(),
            extranonce_prefix: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
                .try_into()
                .unwrap(),
            extranonce_size: 44, // bad extranonce size
        },
    ));
    let replace_message = ReplaceMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
        message_replacement,
    );

    // this sniffer will replace OpenExtendedMiningChannelSuccess with a bad extranonce size
    let (sniffer, sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, Some(replace_message.into())).await;

    let (_, tproxy_addr) = start_sv2_translator(sniffer_addr).await;

    // make sure tProxy shut down (expected behavior)
    // we only assert that the listening port is now available
    assert!(tokio::net::TcpListener::bind(tproxy_addr).await.is_ok());
}
