use integration_tests_sv2::*;

#[tokio::test]
async fn test_interop_proxy_with_multi_ups_extended() {
    start_tracing();
    let (_, tp_addr) = start_template_provider(None);
    let (_pool_1, pool_addr_1) = start_pool(Some(tp_addr)).await;
    let (_pool_2, pool_addr_2) = start_pool(Some(tp_addr)).await;
    let mining_proxy_sv2_addr = start_mining_sv2_proxy(&[pool_addr_1, pool_addr_2]).await;
    let _ = start_mining_device_sv1(mining_proxy_sv2_addr, false, None).await;

    // Need a way to kill pool. And then test if mining_proxy_sv2_addr still binds or not.
    // Need a way to kill MD. And then test if mining_proxy_sv2_addr still binds or not.
}
