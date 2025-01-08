#!/bin/bash

# Check if the active toolchain is nightly
if ! rustup show active-toolchain | grep -q "nightly"; then
  echo "The Rust nightly toolchain is not active. Activate it with:"
  echo "    rustup default nightly"
  exit 1
fi

for manifest in \
  benches/Cargo.toml \
  common/Cargo.toml \
  protocols/Cargo.toml \
  roles/Cargo.toml \
  utils/Cargo.toml \
  utils/message-generator/Cargo.toml; do
    cargo fmt --all --manifest-path=$manifest -- --check
done
