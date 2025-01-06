#!/bin/bash
for manifest in \
  benches/Cargo.toml \
  common/Cargo.toml \
  protocols/Cargo.toml \
  roles/Cargo.toml \
  utils/Cargo.toml \
  utils/message-generator/Cargo.toml; do
    cargo fmt --all --manifest-path=$manifest -- --check
done
