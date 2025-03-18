use std::time::Duration;

use binary_sv2::{B032, U256};
// This file contains integration tests for the `JDC/S` module.
//
// `JDC/S` are modules that implements the Job Decleration roles in the Stratum V2 protocol.
use const_sv2::{
    MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN, MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    MESSAGE_TYPE_DECLARE_MINING_JOB, MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS, MESSAGE_TYPE_SETUP_CONNECTION,
    MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS, MESSAGE_TYPE_SUBMIT_SOLUTION_JD,
};
use integration_tests_sv2::{
    sniffer::{MessageDirection, ReplaceMessage},
    *,
};
use roles_logic_sv2::{job_declaration_sv2::SubmitSolutionJd, parsers::AnyMessage};

use roles_logic_sv2::parsers::CommonMessages;

// This test verifies that jd-server does not exit when a connected jd-client shuts down.
//
// It is performing the verification by shutding down a jd-client connected to a jd-server and then
// starting a new jd-client that connects to the same jd-server successfully.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (jdc, jdc_addr) = start_jdc(&[(pool_addr, jds_addr)], tp_addr).await;
    jdc.shutdown();
    // wait for shutdown to complete
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc_1, _jdc_addr_1) = start_jdc(&[(pool_addr, sniffer_addr)], tp_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}

// This test verifies that jd-client exchange SetupConnection messages with a Template Provider.
//
// Note that jd-client starts to exchange messages with the Template Provider after it has accepted
// a downstream connection.
#[tokio::test]
async fn jdc_tp_success_setup() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (tp_jdc_sniffer, tp_jdc_sniffer_addr) =
        start_sniffer("0".to_string(), tp_addr, false, None).await;
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, jds_addr)], tp_jdc_sniffer_addr).await;
    // This is needed because jd-client waits for a downstream connection before it starts
    // exchanging messages with the Template Provider.
    start_sv2_translator(jdc_addr).await;
    tp_jdc_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    tp_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
}

/// This test ensures that `jd-client` does not panic even if `jd-server` leaves the connection open
/// after receiving the request for token.
///
/// The test verifies whether `jdc` has crashed by attempting to bind to the `jdc` port after 3
/// seconds of no response from `jd-server`.
#[tokio::test]
async fn jdc_does_not_stackoverflow_when_no_token() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let block_from_message = sniffer::IgnoreMessage::new(
        sniffer::MessageDirection::ToDownstream,
        MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
    );
    let (jds_jdc_sniffer, jds_jdc_sniffer_addr) = start_sniffer(
        "JDS-JDC-sniffer".to_string(),
        jds_addr,
        false,
        Some(block_from_message.into()),
    )
    .await;
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, jds_jdc_sniffer_addr)], tp_addr).await;
    let _ = start_sv2_translator(jdc_addr).await;
    jds_jdc_sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    jds_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;

    jds_jdc_sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        )
        .await;

    // The 3-second delay simulates a scenario where JDC does not receive an
    // `AllocateMiningJobTokenSuccess` response from JDS, leaving `self.allocated_tokens` empty.
    // Without the fix introduced in [PR](https://github.com/stratum-mining/stratum/pull/720),
    // JDC would recursively call `Self::get_last_token`, eventually causing a stack overflow.
    // This test verifies that JDC now blocks/yields correctly instead of infinitely recursing.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_err());
}

// This test verifies that JDS does not exit when it receives a `SubmitSolution`
// while still expecting a `ProvideMissingTransactionsSuccess`.
//
// It is performing the verification by connecting to JDS after the message exchange
// to check whether it remains alive.
#[tokio::test]
async fn jds_receive_solution_while_processing_declared_job_test() {
    start_tracing();
    let (tp_1, tp_addr_1) = start_template_provider(None);

    let (tp_2, tp_addr_2) = start_template_provider(None);

    let (_pool, pool_addr) = start_pool(Some(tp_addr_1)).await;
    let (_jds, jds_addr) = start_jds(tp_1.rpc_info()).await;

    // Dummy data to construct a SubmitSolution message
    let prev_hash = U256::Owned(vec![
        184, 103, 138, 88, 153, 105, 236, 29, 123, 246, 107, 203, 1, 33, 10, 122, 188, 139, 218,
        141, 62, 177, 158, 101, 125, 92, 214, 150, 199, 220, 29, 8,
    ]);

    let extranonce = B032::Owned(vec![
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0,
        0, 0,
    ]);

    let submit_solution_replace = ReplaceMessage::new(
        MessageDirection::ToUpstream,
        MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        AnyMessage::JobDeclaration(roles_logic_sv2::parsers::JobDeclaration::SubmitSolution(
            SubmitSolutionJd {
                ntime: 0,
                nbits: 0,
                nonce: 0,
                version: 0,
                prev_hash,
                extranonce,
            },
        )),
    );

    // This sniffer sits between `jds` and `jdc`, replacing `ProvideMissingTransactionSuccess`
    // with `SubmitSolution`.
    let (sniffer_a, sniffer_a_addr) = start_sniffer(
        "A".to_string(),
        jds_addr,
        false,
        Some(submit_solution_replace.into()),
    )
    .await;

    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, sniffer_a_addr)], tp_addr_2).await;

    start_sv2_translator(jdc_addr).await;

    assert!(tp_2.fund_wallet().is_ok());
    assert!(tp_2.create_mempool_transaction().is_ok());

    sniffer_a
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        )
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        )
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_DECLARE_MINING_JOB,
        )
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
        )
        .await;

    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_SUBMIT_SOLUTION_JD,
        )
        .await;

    assert!(tokio::net::TcpListener::bind(jds_addr).await.is_err());
}
