// This file contains integration tests for the `JDC/S` module.
use integration_tests_sv2::{
    interceptor::{MessageDirection, ReplaceMessage},
    template_provider::DifficultyLevel,
    *,
};
use stratum_common::roles_logic_sv2::{
    self,
    codec_sv2::binary_sv2::{Seq064K, B032, U256},
    common_messages_sv2::*,
    job_declaration_sv2::{ProvideMissingTransactionsSuccess, PushSolution, *},
    parsers_sv2::AnyMessage,
};

// This test verifies that jd-server does not exit when a connected jd-client shuts down.
//
// It is performing the verification by shutding down a jd-client connected to a jd-server and then
// starting a new jd-client that connects to the same jd-server successfully.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (sniffer_a, sniffer_addr_a) = start_sniffer("0", jds_addr, false, vec![], None);
    let (jdc, jdc_addr) = start_jdc(&[(pool_addr, sniffer_addr_a)], tp_addr);
    sniffer_a
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
    sniffer_a
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        )
        .await;
    drop(jdc);
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0", jds_addr, false, vec![], None);
    let (_jdc_1, _jdc_addr_1) = start_jdc(&[(pool_addr, sniffer_addr)], tp_addr);
    sniffer
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
        .await;
}

// This test verifies that jd-client exchange SetupConnection messages with a Template Provider.
//
// Note that jd-client starts to exchange messages with the Template Provider after it has accepted
// a downstream connection.
#[tokio::test]
async fn jdc_tp_success_setup() {
    start_tracing();
    let (tp, tp_addr) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info());
    let (tp_jdc_sniffer, tp_jdc_sniffer_addr) = start_sniffer("0", tp_addr, false, vec![], None);
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, jds_addr)], tp_jdc_sniffer_addr);
    // This is needed because jd-client waits for a downstream connection before it starts
    // exchanging messages with the Template Provider.
    start_sv2_translator(jdc_addr);
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

// This test verifies that JDS does not exit when it receives a `SubmitSolution`
// while still expecting a `ProvideMissingTransactionsSuccess`.
//
// It is performing the verification by connecting to JDS after the message exchange
// to check whether it remains alive.
#[tokio::test]
async fn jds_receive_solution_while_processing_declared_job_test() {
    start_tracing();
    let (tp_1, tp_addr_1) = start_template_provider(None, DifficultyLevel::Low);
    let (tp_2, tp_addr_2) = start_template_provider(None, DifficultyLevel::Low);
    let (_pool, pool_addr) = start_pool(Some(tp_addr_1)).await;
    let (_jds, jds_addr) = start_jds(tp_1.rpc_info());

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
        AnyMessage::JobDeclaration(roles_logic_sv2::parsers_sv2::JobDeclaration::PushSolution(
            PushSolution {
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
        "A",
        jds_addr,
        false,
        vec![submit_solution_replace.into()],
        None,
    );
    let (_jdc, jdc_addr) = start_jdc(&[(pool_addr, sniffer_a_addr)], tp_addr_2);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr);
    start_mining_device_sv1(tproxy_addr, true, None);
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
        .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_PUSH_SOLUTION)
        .await;
    assert!(tokio::net::TcpListener::bind(jds_addr).await.is_err());
}

// This test ensures that JDS does not exit upon receiving a `ProvideMissingTransactionsSuccess`
// message containing a transaction set that differs from the `tx_short_hash_list`
// in the Declare Mining Job.
//
// It is performing the verification by connecting to JDS after the message exchange
// to check whether it remains alive
#[tokio::test]
async fn jds_wont_exit_upon_receiving_unexpected_txids_in_provide_missing_transaction_success() {
    start_tracing();
    let (tp_1, tp_addr_1) = start_template_provider(None, DifficultyLevel::Low);
    let (tp_2, tp_addr_2) = start_template_provider(None, DifficultyLevel::Low);

    assert!(tp_2.fund_wallet().is_ok());
    assert!(tp_2.create_mempool_transaction().is_ok());

    let (_pool, pool_addr) = start_pool(Some(tp_addr_1)).await;
    let (_jds, jds_addr) = start_jds(tp_1.rpc_info());

    let provide_missing_transaction_success_replace = ReplaceMessage::new(
        MessageDirection::ToUpstream,
        MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        AnyMessage::JobDeclaration(
            roles_logic_sv2::parsers_sv2::JobDeclaration::ProvideMissingTransactionsSuccess(
                ProvideMissingTransactionsSuccess {
                    request_id: 1,
                    transaction_list: Seq064K::new(Vec::new()).unwrap(),
                },
            ),
        ),
    );

    // This sniffer sits between `jds` and `jdc`, replacing `ProvideMissingTransactionSuccess`
    // with `ProvideMissingTransactionSuccess` with different transaction list.
    let (sniffer, sniffer_addr) = start_sniffer(
        "A",
        jds_addr,
        false,
        vec![provide_missing_transaction_success_replace.into()],
        None,
    );

    let (_, jdc_addr_1) = start_jdc(&[(pool_addr, sniffer_addr)], tp_addr_2);
    let (_translator, tproxy_addr) = start_sv2_translator(jdc_addr_1);
    start_mining_device_sv1(tproxy_addr, false, None);

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
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_DECLARE_MINING_JOB,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
        )
        .await;
    sniffer
        .wait_for_message_type(
            MessageDirection::ToUpstream,
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        )
        .await;

    assert!(tokio::net::TcpListener::bind(jds_addr).await.is_err());
}
