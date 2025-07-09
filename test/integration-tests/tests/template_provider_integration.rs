use integration_tests_sv2::{template_provider::DifficultyLevel, *};

#[tokio::test]
async fn tp_low_diff() {
    let (tp, _) = start_template_provider(None, DifficultyLevel::Low);
    let blockchain_info = tp.get_blockchain_info().unwrap();
    assert_eq!(blockchain_info.difficulty, 4.6565423739069247e-10);
    assert_eq!(blockchain_info.chain, "regtest");
}

#[tokio::test]
async fn tp_mid_diff() {
    let (tp, _) = start_template_provider(None, DifficultyLevel::Mid);
    let blockchain_info = tp.get_blockchain_info().unwrap();
    assert_eq!(blockchain_info.difficulty, 0.001126515290698186);
    assert_eq!(blockchain_info.chain, "signet");
}

#[tokio::test]
async fn tp_high_diff() {
    let (tp, _) = start_template_provider(None, DifficultyLevel::High);
    let blockchain_info = tp.get_blockchain_info().unwrap();
    assert_eq!(blockchain_info.difficulty, 77761.1123986095);
    assert_eq!(blockchain_info.chain, "signet");
}
