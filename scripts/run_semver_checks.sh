#!/bin/bash
for dir in \
  common \
  utils/buffer \
  protocols/v2/binary-sv2/no-serde-sv2/codec \
  protocols/v2/binary-sv2/serde-sv2 \
  protocols/v2/binary-sv2/binary-sv2 \
  protocols/v2/const-sv2 \
  protocols/v2/framing-sv2 \
  protocols/v2/noise-sv2 \
  protocols/v2/codec-sv2 \
  protocols/v2/subprotocols/common-messages \
  protocols/v2/subprotocols/job-declaration \
  protocols/v2/subprotocols/mining \
  protocols/v2/subprotocols/template-distribution \
  protocols/v2/sv2-ffi \
  protocols/v2/roles-logic-sv2 \
  protocols/v1 \
  utils/bip32-key-derivation \
  utils/error-handling \
  utils/key-utils \
  roles/roles-utils/network-helpers \
  roles/roles-utils/rpc; do
    cargo semver-checks --manifest-path="$dir/Cargo.toml"
done
