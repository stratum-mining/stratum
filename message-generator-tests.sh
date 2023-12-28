#! /bin/sh
search_dir="../../test/message-generator/test/"
message_generator_dir="./utils/message-generator/"

cargo llvm-cov clean
cd $message_generator_dir

for entry in `ls $search_dir`; do
    if [ "$entry" = "interop-jdc-change-upstream.json" ]; then
        echo "Skipping $entry"
        continue
    fi

    echo $entry
    cargo run -- $search_dir$entry || { echo 'mg test failed' ; exit 1; }
done

cd ../../
cargo llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report
