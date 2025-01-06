#!/bin/bash
cargo build --manifest-path=benches/Cargo.toml
cargo build --manifest-path=protocols/Cargo.toml
cargo build --manifest-path=roles/Cargo.toml
cargo build --manifest-path=utils/Cargo.toml
