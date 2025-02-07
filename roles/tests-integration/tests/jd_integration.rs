use integration_tests_sv2::*;

use roles_logic_sv2::parsers::{CommonMessages, PoolMessages};

// This test verifies that the `jds` (Job Distributor Server) does not panic when the `jdc`
// (Job Distributor Client) shuts down.
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
    let (_tp, tp_addr) = start_template_provider(None).await;
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp_addr).await;
    let (jdc, jdc_addr) = start_jdc(pool_addr, tp_addr, jds_addr).await;
    jdc.shutdown();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc, _jdc_addr) = start_jdc(pool_addr, tp_addr, sniffer_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}

// This test makes sure that JDS will not stop listening after one
// downstream client disconnects
#[tokio::test]
async fn jds_survives_downstream_disconnect() {
    todo!()
}

// This test makes sure that JDC will not stop listening after one
// downstream client disconnects
#[tokio::test]
async fn jdc_survives_downstream_disconnect() {
    todo!()
}
