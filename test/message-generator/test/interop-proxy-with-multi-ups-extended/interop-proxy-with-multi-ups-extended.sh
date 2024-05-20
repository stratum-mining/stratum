cd roles
cargo llvm-cov --no-report -p pool_sv2
cargo llvm-cov --no-report -p mining_proxy_sv2
cargo build --no-report -p mining-device

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/interop-proxy-with-multi-ups-extended/interop-proxy-with-multi-ups-extended.json || { echo 'mg test failed' ; exit 1; }

sleep 10
