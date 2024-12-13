mod common;

use common::sniffer::MessageDirection;
use const_sv2::{MESSAGE_TYPE_SETUP_CONNECTION, MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED};
use roles_logic_sv2::parsers::{CommonMessages, Mining, PoolMessages};

// This test runs an sv2 translator between an sv1 mining device and a pool. the connection between
// the translator and the pool is intercepted by a sniffer. The test checks if the translator and
// the pool exchange the correct messages upon connection. And that the miner is able to submit
// shares.
#[tokio::test]
async fn translation_proxy() {
    let pool_translator_sniffer_addr = common::get_available_address();
    let tp_addr = common::get_available_address();
    let pool_addr = common::get_available_address();
    let pool_translator_sniffer = common::start_sniffer(
        "0".to_string(),
        pool_translator_sniffer_addr,
        pool_addr,
        false,
        None,
    )
    .await;
    let _tp = common::start_template_provider(tp_addr.port()).await;
    let _pool = common::start_pool(Some(pool_addr), Some(tp_addr)).await;
    let tproxy_addr = common::start_sv2_translator(pool_translator_sniffer_addr).await;
    let _mining_device = common::start_mining_device_sv1(tproxy_addr).await;
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
