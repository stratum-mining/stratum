// This file contains integration tests for the `JDC/S` module.
//
// `JDC/S` are modules that implements the Job Decleration roles in the Stratum V2 protocol.
//
// Note that it is enough to call `start_tracing()` once in the test suite to enable tracing for
// all tests. This is because tracing is a global setting.
use integration_tests_sv2::*;

use roles_logic_sv2::parsers::{CommonMessages, PoolMessages};

// This test verifies that jd-server does not exit when a connected jd-client shuts down.
//
// It is performing the verification by shutding down a jd-client connected to a jd-server and then
// starting a new jd-client that connects to the same jd-server successfully.
#[tokio::test]
async fn jds_should_not_panic_if_jdc_shutsdown() {
    let (tp, tp_addr) = start_template_provider(None);
    let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_jds, jds_addr) = start_jds(tp.rpc_info()).await;
    let (jdc, jdc_addr) = start_jdc(pool_addr, tp_addr, jds_addr).await;
    jdc.shutdown();
    // wait for shutdown to complete
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(tokio::net::TcpListener::bind(jdc_addr).await.is_ok());
    let (sniffer, sniffer_addr) = start_sniffer("0".to_string(), jds_addr, false, None).await;
    let (_jdc_1, _jdc_addr_1) = start_jdc(pool_addr, tp_addr, sniffer_addr).await;
    assert_common_message!(sniffer.next_message_from_downstream(), SetupConnection);
}
