cd roles
cargo build -p translator_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/pool-sri-test-close-channel/pool-sri-test-close-channel.json || { echo 'mg test failed' ; exit 1; }

sleep 10
