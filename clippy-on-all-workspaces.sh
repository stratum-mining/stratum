#!/bin/sh

WORKSPACES="benches/Cargo.toml common/Cargo.toml protocols/Cargo.toml roles/Cargo.toml
utils/Cargo.toml"

for workspace in $WORKSPACES; do
    echo "Executing clippy on: $workspace"
    cargo clippy --manifest-path="$workspace" -- -D warnings -A dead-code
    if [ $? -ne 0 ]; then
        echo "Clippy found some errors in: $workspace"
        exit 1
    fi
done

echo "Clippy success!"

