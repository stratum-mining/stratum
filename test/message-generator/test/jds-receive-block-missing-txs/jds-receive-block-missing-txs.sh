cd roles
cargo llvm-cov --no-report -p jd_server

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/jds-receive-block-missing-txs/jds-receive-block-missing-txs.json || { echo 'mg test failed' ; exit 1; }

sleep 10
