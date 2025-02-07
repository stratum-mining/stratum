use const_sv2::{
    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES, MESSAGE_TYPE_SETUP_CONNECTION,
    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
};
use integration_tests_sv2::{sniffer::*, *};
use roles_logic_sv2::{
    mining_sv2::OpenExtendedMiningChannelSuccess,
    parsers::{CommonMessages, Mining, PoolMessages},
};
use std::convert::TryInto;

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[tokio::test]
async fn translation_proxy() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (pool_translator_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None).await;
    let (_, tproxy_addr) = start_sv2_translator(pool_translator_sniffer_addr).await;
    let _mining_device = start_mining_device_sv1(tproxy_addr).await;
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

// when a tProxy receives a OpenExtendedMiningChannelSuccess with a bad extranonce_size
// we expect it to shut down gracefully
#[tokio::test]
async fn tproxy_refuses_bad_extranonce_size() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;

    let message_replacement = PoolMessages::Mining(Mining::OpenExtendedMiningChannelSuccess(
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
    let intercept = InterceptMessage::new(
        MessageDirection::ToDownstream,
        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES,
        message_replacement,
    );

    // this sniffer will replace OpenExtendedMiningChannelSuccess with a bad extranonce size
    let (sniffer, sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, Some(vec![intercept])).await;

    let (_, tproxy_addr) = start_sv2_translator(sniffer_addr).await;

    // make sure tProxy shut down (expected behavior)
    // we only assert that the listening port is now available
    assert!(tokio::net::TcpListener::bind(tproxy_addr).await.is_ok());
}
