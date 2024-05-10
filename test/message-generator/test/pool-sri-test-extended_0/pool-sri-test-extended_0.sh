cd roles
cargo llvm-cov --no-report -p pool_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/pool-sri-test-extended_0/pool-sri-test-extended_0.json || { echo 'mg test failed' ; exit 1; }
