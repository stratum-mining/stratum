#!/bin/bash

git fetch --all

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
"roles/jd-client"
"roles/jd-server"
"roles/mining-proxy"
"roles/pool"
"roles/translator"
)

# Loop through each crate
for crate in "${crates[@]}"; do
  cd "$crate"

  # Check if the branches exist locally, if not, create them
  git show-ref --verify --quiet refs/remotes/origin/main || { echo "Branch 'main' not found."; exit 1; }
  git show-ref --verify --quiet refs/remotes/origin/dev || { echo "Branch 'dev' not found."; exit 1; }

  # Check if there were any changes between dev and main
  git diff --quiet "origin/dev" "origin/main" -- .
  if [ $? -ne 0 ]; then

      # Check if crate versions on dev and main are identical
      version_dev=$(git show origin/dev:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
      version_main=$(git show origin/main:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
      if [ "$version_dev" = "$version_main" ]; then
         # this prevents the release PR from being merged, since we do `exit 1`, effectively stopping the Github CI
         echo "Changes detected in crate $crate between dev and main branches! Versions on dev and main branches are identical ($version_dev), so you should bump the crate version on dev before merging into main."
         exit 1
      else
         # this creates a log of version changes, useful for release logs
         echo "Changes detected in crate $crate between dev and main branches! Version in dev is: ($version_dev), while version in main is ($version_main)."
      fi
  fi

  cd - >/dev/null
done