cd roles
cargo build -p translator_sv2
cargo build -p mining_device_sv1

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/translation-proxy/translation-proxy.json || { echo 'mg test failed' ; exit 1; }

sleep 10
