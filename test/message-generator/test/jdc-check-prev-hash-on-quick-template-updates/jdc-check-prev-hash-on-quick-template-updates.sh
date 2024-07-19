cd roles
cargo llvm-cov --no-report -p jd_client
cargo llvm-cov --no-report -p pool_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/jdc-check-prev-hash-on-quick-template-updates/jdc-check-prev-hash-on-quick-template-updates.json || { echo 'mg test failed' ; exit 1; }

sleep 10
