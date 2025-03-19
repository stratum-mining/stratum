#!/bin/bash

workspaces=("benches" "common" "roles" "protocols" "utils" "test/integration-tests")

# print current rust version
echo "Rust version: $(rustc --version)"

for workspace in "${workspaces[@]}"; do
  echo "Running clippy for workspace: $workspace"
  cargo clippy --manifest-path="$workspace/Cargo.toml" -- -D warnings
  if [[ $? -ne 0 ]]; then
    echo "Clippy failed for workspace: $workspace"
    exit 1
  else
    echo "Clippy passed for workspace: $workspace"
  fi
done

echo "Clippy success!"
exit 0
