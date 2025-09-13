// This file contains integration tests for the `PoolSv2` module.
//
// `PoolSv2` is a module that implements the Pool role in the Stratum V2 protocol.
use integration_tests_sv2::{interceptor::MessageDirection, template_provider::DifficultyLevel, *};
use stratum_common::roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection, *},
    mining_sv2::*,
    parsers_sv2::{AnyMessage, CommonMessages, Mining, TemplateDistribution},
    template_distribution_sv2::*,
};

// This test starts a Template Provider and a Pool, and checks if they exchange the correct
// messages upon connection.
// The Sniffer is used as a proxy between the Upstream(Template Provider) and Downstream(Pool). The
// Pool will connect to the Sniffer, and the Sniffer will connect to the Template Provider.
#[tokio::test]
async fn success_pool_template_provider_connection() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (sniffer, sniffer_addr) = start_sniffer("", tp_addr, true, vec![], None);
    let _ = start_pool(Some(sniffer_addr)).await;
    // here we assert that the downstream(pool in this case) have sent `SetupConnection` message
    // with the correct parameters, protocol, flags, min_version and max_version.  Note that the
    // macro can take any number of arguments after the message argument, but the order is
    // important where a property should be followed by its value.
    sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
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
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    assert_common_message!(
        &sniffer.next_message_from_upstream(),
        SetupConnectionSuccess
    );
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
        )
        .await;
    assert_tp_message!(
        &sniffer.next_message_from_downstream(),
        CoinbaseOutputConstraints
    );
    sniffer
        .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE)
        .await;
    assert_tp_message!(&sniffer.next_message_from_upstream(), NewTemplate);
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SET_NEW_PREV_HASH,
        )
        .await;
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
    start_tracing();
    let sv2_interval = Some(5);
    let (_tp, tp_addr) = start_template_provider(sv2_interval, DifficultyLevel::High);
    let tp_pool_sniffer_identifier =
        "header_timestamp_value_assertion_in_new_extended_mining_job tp_pool sniffer";
    let (tp_pool_sniffer, tp_pool_sniffer_addr) =
        start_sniffer(tp_pool_sniffer_identifier, tp_addr, false, vec![], None);
    let (_, pool_addr) = start_pool(Some(tp_pool_sniffer_addr)).await;
    let pool_translator_sniffer_identifier =
        "header_timestamp_value_assertion_in_new_extended_mining_job pool_translator sniffer";
    let (pool_translator_sniffer, pool_translator_sniffer_addr) = start_sniffer(
        pool_translator_sniffer_identifier,
        pool_addr,
        false,
        vec![
            // Block SubmitSharesSuccess messages to prevent them from interfering
            // with the test's expectation to receive NewExtendedMiningJob messages
            integration_tests_sv2::interceptor::IgnoreMessage::new(
                integration_tests_sv2::interceptor::MessageDirection::ToDownstream,
                MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            )
            .into(),
        ],
        None,
    );
    let (_tproxy, tproxy_addr) = start_sv2_translator(pool_translator_sniffer_addr);
    start_mining_device_sv1(tproxy_addr, true, None);

    tp_pool_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
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
    pool_translator_sniffer
        .wait_for_message_type_and_clean_queue(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        )
        .await;
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

// This test starts a Pool, a Sniffer, and a Sv2 Mining Device.  It then checks if the Pool receives
// a share from the Sv2 Mining Device.  While also checking all the messages exchanged between the
// Pool and the Mining Device in between.
#[tokio::test]
async fn pool_standard_channel_receives_share() {
    start_tracing();
    let (_tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (sniffer, sniffer_addr) = start_sniffer("A", pool_addr, false, vec![], None);
    start_mining_device_sv2(sniffer_addr, None, None, None, 1, None, true);
    sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
        )
        .await;

    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        )
        .await;
    sniffer
        .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_MINING_JOB)
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        )
        .await;
}
