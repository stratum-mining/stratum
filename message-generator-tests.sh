#! /bin/sh

if [ "$1" = "--short" ]; then
    RUST_LOG="info"
else
    RUST_LOG="debug"
fi

search_dir="../../test/message-generator/test/"
message_generator_dir="./utils/message-generator/"

cd $message_generator_dir
cargo llvm-cov clean

for entry in `ls $search_dir`; do
    if [ "$entry" = "interop-jdc-change-upstream.json" ]; then
        echo "Skipping $entry"
        continue
    fi

    echo $entry
    RUST_LOG=$RUST_LOG cargo run -- $search_dir$entry || { echo 'mg test failed' ; exit 1; }
done

cd ../../roles
RUST_LOG=$RUST_LOG cargo llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report
