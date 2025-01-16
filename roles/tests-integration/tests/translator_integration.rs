use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED};
use integration_tests_sv2::{sniffer::*, *};
use roles_logic_sv2::parsers::{CommonMessages, Mining, PoolMessages};

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[ignore]
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

#[tokio::test]
async fn translation_proxy_and_jd() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (jdc_pool_sniffer, pool_translator_sniffer_addr) =
        start_sniffer("0".to_string(), pool_addr, false, None).await;
    let (_jds, jds_addr) = start_jds(tp_addr).await;
    let (_jdc, jdc_addr) = start_jdc(pool_translator_sniffer_addr, tp_addr, jds_addr).await;
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr).await;
    let _mining_device = start_mining_device_sv1(tproxy_addr).await;
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
	dbg!("before wait for submit shares");
    jdc_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        )
        .await;
	dbg!("after wait for submit shares");
}
