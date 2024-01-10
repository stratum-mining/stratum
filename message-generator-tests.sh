#! /bin/sh
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
    cargo run -- $search_dir$entry || { echo 'mg test failed' ; exit 1; }
done

cd ../../roles
cargo llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report
