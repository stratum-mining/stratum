use integration_tests_sv2::*;

use crate::sniffer::MessageDirection;
use const_sv2::{MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB, MESSAGE_TYPE_NEW_TEMPLATE};
use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    parsers::{AnyMessage, CommonMessages, Mining, PoolMessages, TemplateDistribution},
};

// This test starts a Template Provider and a Pool, and checks if they exchange the correct
// messages upon connection.
// The Sniffer is used as a proxy between the Upstream(Template Provider) and Downstream(Pool). The
// Pool will connect to the Sniffer, and the Sniffer will connect to the Template Provider.
#[tokio::test]
async fn success_pool_template_provider_connection() {
    let (_tp, tp_addr) = start_template_provider(None).await;
    let (sniffer, sniffer_addr) = start_sniffer("".to_string(), tp_addr, true, None).await;
    let _ = start_pool(Some(sniffer_addr)).await;
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
    sniffer
        .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE)
        .await;
    assert_tp_message!(&sniffer.next_message_from_upstream(), NewTemplate);
    assert_tp_message!(sniffer.next_message_from_upstream(), SetNewPrevHash);
}

// This test starts a Template Provider, a Pool, and a Translator Proxy, and verifies the
// correctness of the exchanged messages during connection and operation.
//
// Two Sniffers are used:
// - Between the Template Provider and the Pool.
// - Between the Pool and the Translator Proxy.
//
// The test ensures that:
// - The Template Provider sends valid `SetNewPrevHash` and `NewTemplate` messages.
// - The `minntime` field in the second `NewExtendedMiningJob` message sent to the Translator Proxy
//   matches the `header_timestamp` from the `SetNewPrevHash` message, addressing a bug that
//   occurred with non-future jobs.
//
// Related issue: https://github.com/stratum-mining/stratum/issues/1324
#[tokio::test]
async fn header_timestamp_value_assertion_in_new_extended_mining_job() {
    let sv2_interval = Some(5);
    let (_tp, tp_addr) = start_template_provider(sv2_interval).await;
    let tp_pool_sniffer_identifier =
        "header_timestamp_value_assertion_in_new_extended_mining_job tp_pool sniffer".to_string();
    let (tp_pool_sniffer, tp_pool_sniffer_addr) =
        start_sniffer(tp_pool_sniffer_identifier, tp_addr, false, None).await;
    let (_, pool_addr) = start_pool(Some(tp_pool_sniffer_addr)).await;
    let pool_translator_sniffer_identifier =
        "header_timestamp_value_assertion_in_new_extended_mining_job pool_translator sniffer"
            .to_string();
    let (pool_translator_sniffer, pool_translator_sniffer_addr) =
        start_sniffer(pool_translator_sniffer_identifier, pool_addr, false, None).await;
    let _tproxy_addr = start_sv2_translator(pool_translator_sniffer_addr).await;
    assert_common_message!(
        &tp_pool_sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    // Wait for a NewTemplate message from the Template Provider
    tp_pool_sniffer
        .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE)
        .await;
    assert_tp_message!(&tp_pool_sniffer.next_message_from_upstream(), NewTemplate);
    // Extract header timestamp from SetNewPrevHash message
    let header_timestamp_to_check = match tp_pool_sniffer.next_message_from_upstream() {
        Some((_, AnyMessage::TemplateDistribution(TemplateDistribution::SetNewPrevHash(msg)))) => {
            msg.header_timestamp
        }
        _ => panic!("SetNewPrevHash not found!"),
    };
    // Assertions of messages between Pool and Translator Proxy (these are not necessary for the
    // test itself, but they are used to pop from the sniffer's message queue)
    assert_common_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        OpenExtendedMiningChannelSuccess
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        NewExtendedMiningJob
    );
    assert_mining_message!(
        &pool_translator_sniffer.next_message_from_upstream(),
        SetNewPrevHash
    );
    // Wait for a second NewExtendedMiningJob message
    pool_translator_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
        )
        .await;
    // Extract min_ntime from the second NewExtendedMiningJob message
    let second_job_ntime = match pool_translator_sniffer.next_message_from_upstream() {
        Some((_, AnyMessage::Mining(Mining::NewExtendedMiningJob(job)))) => {
            job.min_ntime.into_inner()
        }
        _ => panic!("Second NewExtendedMiningJob not found!"),
    };
    // Assert that min_ntime matches header_timestamp
    assert_eq!(
        second_job_ntime,
        Some(header_timestamp_to_check),
        "The `minntime` field of the second NewExtendedMiningJob does not match the `header_timestamp`!"
    );
}
