cd roles
cargo build -p pool_sv2
cargo build -p translator_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/sv1-test/sv1-test.json || { echo 'mg test failed' ; exit 1; }

sleep 10
