#!/bin/bash
for manifest in \
  benches/Cargo.toml \
  common/Cargo.toml \
  protocols/Cargo.toml \
  roles/Cargo.toml \
  utils/Cargo.toml \
  utils/message-generator/Cargo.toml; do
    cargo clippy --manifest-path=$manifest -- -D warnings -A dead-code
done
