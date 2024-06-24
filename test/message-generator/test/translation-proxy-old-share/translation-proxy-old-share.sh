cd roles

cargo build -p translator_sv2
cargo build -p

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/translation-proxy-old-share/translation-proxy-old-share.json || { echo 'mg test failed' ; exit 1; }

sleep 10
