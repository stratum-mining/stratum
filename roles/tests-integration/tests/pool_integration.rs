mod common;

use std::{convert::TryInto, time::Duration};

use common::{InterceptMessage, MessageDirection};
use const_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR;
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, SetupConnectionError},
    parsers::{CommonMessages, Mining, PoolMessages, TemplateDistribution},
};
use tokio::time::sleep;

// This test starts a Template Provider and a Pool, and checks if they exchange the correct
// messages upon connection.
// The Sniffer is used as a proxy between the Upstream(Template Provider) and Downstream(Pool). The
// Pool will connect to the Sniffer, and the Sniffer will connect to the Template Provider.
#[tokio::test]
async fn success_pool_template_provider_connection() {
    let sniffer_addr = common::get_available_address();
    let tp_addr = common::get_available_address();
    let pool_addr = common::get_available_address();
    let _tp = common::start_template_provider(tp_addr.port()).await;
    let sniffer_identifier =
        "success_pool_template_provider_connection tp_pool sniffer".to_string();
    let sniffer_check_on_drop = true;
    let sniffer = common::start_sniffer(
        sniffer_identifier,
        sniffer_addr,
        tp_addr,
        sniffer_check_on_drop,
        None,
    )
    .await;
    let _ = common::start_pool(Some(pool_addr), Some(sniffer_addr)).await;
    // here we assert that the downstream(pool in this case) have sent `SetupConnection` message
    // with the correct parameters, protocol, flags, min_version and max_version.  Note that the
    // macro can take any number of arguments after the message argument, but the order is
    // important where a property should be followed by its value.
    assert_common_message!(
        &sniffer.next_message_from_downstream(),
        SetupConnection,
        protocol,
        Protocol::TemplateDistributionProtocol,
        flags,
        0,
        min_version,
        2,
        max_version,
        2
    );
    assert_common_message!(
        &sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    assert_tp_message!(
        &sniffer.next_message_from_downstream(),
        CoinbaseOutputDataSize
    );
    assert_tp_message!(&sniffer.next_message_from_upstream(), NewTemplate);
    assert_tp_message!(sniffer.next_message_from_upstream(), SetNewPrevHash);
}

#[tokio::test]
async fn test_sniffer_interrupter() {
    let sniffer_addr = common::get_available_address();
    let tp_addr = common::get_available_address();
    let pool_addr = common::get_available_address();
    let _tp = common::start_template_provider(tp_addr.port()).await;
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
    let sniffer = common::start_sniffer(
        "1".to_string(),
        sniffer_addr,
        tp_addr,
        false,
        Some(vec![interrupt_msgs]),
    )
    .await;
    let _ = common::start_pool(Some(pool_addr), Some(sniffer_addr)).await;
    assert_common_message!(&sniffer.next_message_from_downstream(), SetupConnection);
    assert_common_message!(&sniffer.next_message_from_upstream(), SetupConnectionError);
}

// covers
// https://github.com/stratum-mining/stratum/blob/main/test/message-generator/test/translation-proxy/translation-proxy.json
#[tokio::test]
async fn translation_proxy() {
    let pool_jdc_sniffer_addr = common::get_available_address();
    let tp_addr = common::get_available_address();
    let pool_addr = common::get_available_address();

    let pool_jdc_sniffer = common::start_sniffer(
        "0".to_string(),
        pool_jdc_sniffer_addr,
        pool_addr,
        false,
        None,
    )
    .await;
    let _tp = common::start_template_provider(tp_addr.port()).await;
    let _pool_1 = common::start_pool(Some(pool_addr), Some(tp_addr)).await;
    let jds_addr = common::start_jds(tp_addr).await;
    let jdc_addr = common::start_jdc(pool_jdc_sniffer_addr, tp_addr, jds_addr).await;
    let mining_proxy_addr = common::start_sv2_translator(jdc_addr).await;
    let _ = common::start_mining_device_sv1(mining_proxy_addr).await;
    sleep(Duration::from_secs(3)).await;

    assert_common_message!(
        &pool_jdc_sniffer.next_message_from_downstream(),
        SetupConnection
    );
    assert_common_message!(
        &pool_jdc_sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_downstream(),
        OpenExtendedMiningChannel
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_upstream(),
        OpenExtendedMiningChannelSuccess
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_upstream(),
        NewExtendedMiningJob
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_downstream(),
        SetCustomMiningJob
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_upstream(),
        SetNewPrevHash
    );
    assert_mining_message!(
        &pool_jdc_sniffer.next_message_from_downstream(),
        SubmitSharesExtended
    );
}
