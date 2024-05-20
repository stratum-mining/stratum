cd roles
cargo llvm-cov --no-report -p jd_server

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/jds-do-not-panic-if-jdc-close-connection/jds-do-not-panic-if-jdc-close-connection.json || { echo 'mg test failed' ; exit 1; }

sleep 10
