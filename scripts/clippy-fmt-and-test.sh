#!/bin/sh

WORKSPACES="benches common protocols roles utils"

for workspace in $WORKSPACES; do
    echo "Executing clippy on: $workspace"
    cargo +1.75.0 clippy --manifest-path="$workspace/Cargo.toml" -- -D warnings -A dead-code
    if [ $? -ne 0 ]; then
        echo "Clippy found some errors in: $workspace"
        exit 1
    fi

    echo "Running tests on: $workspace"
    cargo +1.75 test --manifest-path="$workspace/Cargo.toml"
    if [ $? -ne 0 ]; then
        echo "Tests failed in: $workspace"
        exit 1
    fi

    echo "Running fmt on: $workspace"
    (cd $workspace && cargo +nightly fmt)
    if [ $? -ne 0 ]; then
        echo "Fmt failed in: $workspace"
        exit 1
    fi
done

echo "Clippy success, all tests passed!"
