#! /bin/sh

search_dir="test/message-generator/test/"

for entry in `ls $search_dir`; do
    echo $entry
    $search_dir$entry/$entry.sh
done

cd roles
RUST_LOG=debug cargo +1.75.0 llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report

