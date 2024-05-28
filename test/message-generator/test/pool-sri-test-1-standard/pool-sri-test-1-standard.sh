cd roles
cargo llvm-cov --no-report -p pool_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/pool-sri-test-1-standard/pool-sri-test-1-standard.json || { echo 'mg test failed' ; exit 1; }

sleep 10
