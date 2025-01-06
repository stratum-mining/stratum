#!/bin/bash

# Instala toolchain e componentes
cargo build --manifest-path=benches/Cargo.toml
cargo build --manifest-path=common/Cargo.toml
cargo build --manifest-path=protocols/Cargo.toml
cargo build --manifest-path=roles/Cargo.toml
cargo build --manifest-path=utils/Cargo.toml

# Testes de Integração de Roles
cargo test --manifest-path=roles/Cargo.toml --verbose --test '*' -- --nocapture

# Executa o exemplo de client_and_server
cargo run --manifest-path=examples/sv1-client-and-server/Cargo.toml --bin client_and_server -- 60

# Teste de Interop (somente no Ubuntu)
if [ "$CI_OS" == "ubuntu-latest" ]; then
    ./run.sh 30
else
    echo "Skipping interop-test on $CI_OS - not supported"
fi
cd examples/interop-cpp/ || exit

# Teste de fuzz (somente no Ubuntu)
if [ "$CI_OS" == "ubuntu-latest" ]; then
    ./run.sh 30
else
    echo "Skipping fuzz test on $CI_OS - not supported"
fi
cd utils/buffer/fuzz || exit

# Testes gerais
cargo test --manifest-path=benches/Cargo.toml
cargo test --manifest-path=common/Cargo.toml
cargo test --manifest-path=protocols/Cargo.toml
cargo test --manifest-path=roles/Cargo.toml
cargo test --manifest-path=utils/Cargo.toml

# Testes baseados em propriedades
cargo test --manifest-path=protocols/Cargo.toml --features prop_test

# Executa o exemplo de ping-pong-with-noise
cargo run --manifest-path=examples/ping-pong-with-noise/Cargo.toml --bin ping_pong_with_noise -- 10

# Executa o exemplo de ping-pong-without-noise
cargo run --manifest-path=examples/ping-pong-without-noise/Cargo.toml --bin ping_pong_without_noise -- 10
