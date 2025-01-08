#!/bin/bash

for manifest in \
  benches/Cargo.toml \
  common/Cargo.toml \
  protocols/Cargo.toml \
  roles/Cargo.toml \
  utils/Cargo.toml; do
    cargo test --manifest-path="$manifest"
done

cargo test --manifest-path=protocols/Cargo.toml --features prop_test
