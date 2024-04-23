#! /bin/sh

if [ "$1" = "--short" ]; then
    RUST_LOG="info"
else
    RUST_LOG="debug"
fi

message_generator_dir="./utils/message-generator/"
cd $message_generator_dir
cargo llvm-cov clean

cd ../../roles
cargo build -p mining-device
cargo llvm-cov -p pool_sv2
cargo llvm-cov -p jd_server
cargo llvm-cov -p jd_client
cargo llvm-cov -p translator_sv2 
cargo llvm-cov -p mining_proxy_sv2 
cd ./roles-utils
cargo llvm-cov 
cd ../../

search_dir="../../test/message-generator/test/"

cd $message_generator_dir
cargo build

for entry in `ls $search_dir`; do
    if [ "$entry" = "interop-jdc-change-upstream.json" ]; then
        echo "Skipping $entry"
        continue
    fi

    sleep 10

    echo $entry
    RUST_LOG=$RUST_LOG cargo run -- $search_dir$entry || { echo 'mg test failed' ; exit 1; }
done

cd ../../roles
RUST_LOG=$RUST_LOG cargo llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report
