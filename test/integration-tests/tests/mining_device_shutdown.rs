use integration_tests_sv2::*;


// Tests whether JDC will switch to a new pool after receiving a `SubmitSharesError` message from
// the currently connected pool.
#[tokio::test]
async fn test_jdc_pool_fallback_after_submit_rejection() {
    start_tracing();
    let (_, tp_addr) = start_template_provider(None);
    let (_pool_1, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_translator, sv2_translator_addr) = start_sv2_translator(pool_addr).await;
    let mining_device_shutdown_handler = start_mining_device_sv1(sv2_translator_addr, true, None).await;

    mining_device_shutdown_handler.send(()).unwrap();
}
