cd roles
cargo llvm-cov --no-report -p pool_sv2
cargo llvm-cov --no-report -p jd_server
cargo llvm-cov --no-report -p jd_client
cargo llvm-cov --no-report -p translator_sv2
cargo build -p mining_device_sv1

cd ../utils/message-generator/
cargo build

RUST_LOG=debug cargo run ../../test/message-generator/test/interop-jd-translator/interop-jd-translator.json || { echo 'mg test failed' ; exit 1; }

sleep 10
