#!/bin/bash

cargo build --manifest-path=benches/Cargo.toml
cargo build --manifest-path=common/Cargo.toml
cargo build --manifest-path=protocols/Cargo.toml
cargo build --manifest-path=roles/Cargo.toml
cargo build --manifest-path=utils/Cargo.toml

cargo test --manifest-path=roles/Cargo.toml --verbose --test '*' -- --nocapture

cargo run --manifest-path=examples/sv1-client-and-server/Cargo.toml --bin client_and_server -- 60

if [ "$CI_OS" == "ubuntu-latest" ]; then
    ./run.sh 30
else
    echo "Skipping interop-test on $CI_OS - not supported"
fi
cd examples/interop-cpp/ || exit

if [ "$CI_OS" == "ubuntu-latest" ]; then
    ./run.sh 30
else
    echo "Skipping fuzz test on $CI_OS - not supported"
fi

if [ -d "utils/buffer/fuzz" ]; then
    cd utils/buffer/fuzz || exit
else
    echo "Directory 'utils/buffer/fuzz' not found. Skipping..."
fi

cargo test --manifest-path=benches/Cargo.toml
cargo test --manifest-path=common/Cargo.toml
cargo test --manifest-path=protocols/Cargo.toml
cargo test --manifest-path=roles/Cargo.toml
cargo test --manifest-path=utils/Cargo.toml

cargo test --manifest-path=protocols/Cargo.toml --features prop_test

cargo run --manifest-path=examples/ping-pong-with-noise/Cargo.toml --bin ping_pong_with_noise -- 10

cargo run --manifest-path=examples/ping-pong-without-noise/Cargo.toml --bin ping_pong_without_noise -- 10
