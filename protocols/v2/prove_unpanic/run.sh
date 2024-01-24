#! /bin/bash

cargo clean && RUST_BACKTRACE=1 RUSTC_WRAPPER=~/src/unpanic/unpanic TARGET_CRATE=prove_unpanic cargo +nightly-2023-08-25-x86_64-unknown-linux-gnu build -p prove_unpanic
