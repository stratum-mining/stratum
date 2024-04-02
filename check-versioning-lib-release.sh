#!/bin/bash

git fetch origin main
git fetch origin dev

# this list was taken from `.github/workflows/release-libs.yaml`.
# if anything changes there (crates added/removed to publishing pipeline)
# those changes should be reflected here
crates=(
"utils/buffer"
"protocols/v2/binary-sv2/no-serde-sv2/derive_codec"
"protocols/v2/binary-sv2/no-serde-sv2/codec"
"protocols/v2/binary-sv2/serde-sv2"
"protocols/v2/binary-sv2/binary-sv2"
"protocols/v2/const-sv2"
"protocols/v2/framing-sv2"
"protocols/v2/noise-sv2"
"protocols/v2/codec-sv2"
"protocols/v2/subprotocols/common-messages"
"protocols/v2/subprotocols/job-declaration"
"protocols/v2/subprotocols/mining"
"protocols/v2/subprotocols/template-distribution"
"protocols/v2/sv2-ffi"
"protocols/v2/roles-logic-sv2"
"protocols/v1"
"utils/bip32-key-derivation"
"utils/error-handling"
"utils/key-utils"
"roles/roles-utils/network-helpers"
"roles/roles-utils/rpc"
)

# Loop through each crate
for crate in "${crates[@]}"; do
  cd "$crate"

  # Check if there were any changes between dev and main
  git diff --quiet "origin/dev" "origin/main" -- .
  if [ $? -ne 0 ]; then

      # Check if crate versions on dev and main are identical
      version_dev=$(git show origin/dev:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
      version_main=$(git show origin/main:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
      if [ "$version_dev" = "$version_main" ]; then
         echo "Changes detected in crate $crate between dev and main branches! Versions on dev and main branches are identical ($version_dev), so you should bump the crate version on dev before merging into main."
         exit 1
      fi
  fi

  cd - >/dev/null
done