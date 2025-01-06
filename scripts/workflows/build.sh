#!/bin/bash
for manifest in \
  benches/Cargo.toml \
  protocols/Cargo.toml \
  roles/Cargo.toml \
  utils/Cargo.toml; do
    cargo build --manifest-path="$manifest"
done
