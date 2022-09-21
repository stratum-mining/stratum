#! /bin/sh
set -ex

rustup toolchain install nightly
cargo +nightly install cargo-fuzz
cargo +nightly --version
cargo +nightly fuzz run faster -- -rss_limit_mb=5000000000 -runs=1000
cargo +nightly fuzz run slower -- -rss_limit_mb=5000000000 -runs=1000
