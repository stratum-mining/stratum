cd roles
cargo llvm-cov --no-report -p jd_server
cargo llvm-cov --no-report -p jd_client
cargo llvm-cov --no-report -p mining_proxy_sv2
cargo build -p --no-report mining-device

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/interop-jdc-change-upstream/interop-jdc-change-upstream.json || { echo 'mg test failed' ; exit 1; }

sleep 10
