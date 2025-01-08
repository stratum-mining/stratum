#!/bin/bash
for example in \
  "examples/ping-pong-with-noise/Cargo.toml --bin ping_pong_with_noise -- 10" \
  "examples/ping-pong-without-noise/Cargo.toml --bin ping_pong_without_noise -- 10"; do
    # shellcheck disable=SC2046
    cargo run --manifest-path=$(echo "$example" | awk '{print $1}') $(echo "$example" | cut -d' ' -f2-)
done