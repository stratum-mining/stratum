cd roles
cargo build -p translator_sv2

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/translation-proxy-broke-pool/translation-proxy-broke-pool.json || { echo 'mg test failed' ; exit 1; }

sleep 10
