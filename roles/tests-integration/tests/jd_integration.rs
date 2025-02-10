// This file contains integration tests for the `JDC/S` module.
//
// `JDC/S` are modules that implements the Job Decleration roles in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use integration_tests_sv2::*;

use roles_logic_sv2::parsers::{CommonMessages, PoolMessages};

// This test verifies that the `jds` (Job Decleration Server) does not panic when the `jdc`
// (Job Decleration Client) shuts down.
//
// The test follows these steps:
// 1. Start a Template Provider (`tp`) and a Pool.
// 2. Start the `jds` and the `jdc`, ensuring the `jdc` connects to the `jds`.
// 3. Shut down the `jdc` and ensure the `jds` remains operational without panicking.
// 4. Verify that the `jdc`'s address can be reused by asserting that a new `TcpListener` can bind
//    to the same address.
// 5. Start a Sniffer as a proxy for observing messages exchanged between the `jds` and other
//    components.
// 6. Reconnect a new `jdc` to the Sniffer and ensure the expected `SetupConnection` message is
//    received from the `jdc`.
//
// This ensures that the shutdown of the `jdc` is handled gracefully by the `jds`, and subsequent
// connections or operations continue without issues.
//
// # Notes
// - The test waits for a brief duration (`1 second`) after shutting down the `jdc` to allow for any
//   internal cleanup or reconnection attempts.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (jdc, jdc_addr) = start_jdc(pool_addr, tp_addr, jds_addr).await;
    jdc.shutdown();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc, _jdc_addr) = start_jdc(pool_addr, tp_addr, sniffer_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}
