#! /bin/bash

cargo clean && RUSTC_WRAPPER=~/src/unpanic/unpanic TARGET_CRATE=jd_client cargo +nightly-2023-08-25-x86_64-unknown-linux-gnu build
