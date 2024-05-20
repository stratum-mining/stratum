cd roles
cargo llvm-cov --no-report -p pool_sv2
cargo llvm-cov --no-report -p jd_client
cargo llvm-cov --no-report -p mining_proxy_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/jds-do-not-stackoverflow-when-no-token/jds-do-not-stackoverflow-when-no-token.json || { echo 'mg test failed' ; exit 1; }

sleep 10
