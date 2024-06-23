#! /bin/sh

cd roles
RUST_LOG=debug cargo llvm-cov --ignore-filename-regex "utils/message-generator/|experimental/|protocols/" --cobertura --output-path "target/mg_coverage.xml" report
