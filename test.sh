#!/bin/bash

# exit on first error, see: http://stackoverflow.com/a/185900/432509
error() {
    local parent_lineno="$1"
    local message="$2"
    local code="${3:-1}"
    if [[ -n "$message" ]] ; then
        echo "Error on or near line ${parent_lineno}: ${message}; exiting with status ${code}"
    else
        echo "Error on or near line ${parent_lineno}; exiting with status ${code}"
    fi
    exit "${code}"
}
trap 'error ${LINENO}' ERR
# done with trap

# Support cargo command override
if [[ -z $CARGO_BIN ]]; then
    CARGO_BIN=cargo
fi

# Top level of this git repository
ROOT=$(git rev-parse --show-toplevel)

# Loop through each crate and execute the tests
# Each crate path is found by searching for a Cargo.toml file
for cargo_dir in $(find . -type f -name 'Cargo.toml' | sed -r 's|/[^/]+$||'); do
    echo Running tests in: $cargo_dir
    cd $cargo_dir
    # Compile the tests, but do not execute
    RUST_BACKTRACE=0 $CARGO_BIN test --no-run
    # Execute tests and print any outputs to stdout
    RUST_BACKTRACE=1 $CARGO_BIN test -- --nocapture
    cd $ROOT
done

rm ./protocols/v2/binary-sv2/binary-sv2/Cargo.lock
rm -r ./protocols/v2/binary-sv2/binary-sv2/target/
rm ./protocols/v2/binary-sv2/no-serde-sv2/codec/Cargo.lock
rm -r ./protocols/v2/binary-sv2/no-serde-sv2/codec/target/
rm ./protocols/v2/binary-sv2/no-serde-sv2/derive_codec/Cargo.lock
rm -r ./protocols/v2/binary-sv2/no-serde-sv2/derive_codec/target/
rm ./protocols/v2/codec-sv2/Cargo.lock
rm -r ./protocols/v2/codec-sv2/target/
rm ./protocols/v2/const-sv2/Cargo.lock
rm -r ./protocols/v2/const-sv2/target/
rm ./protocols/v2/framing-sv2/Cargo.lock
rm -r ./protocols/v2/framing-sv2/target/
rm ./protocols/v2/subprotocols/common-messages/Cargo.lock
rm -r ./protocols/v2/subprotocols/common-messages/target/
rm ./protocols/v2/subprotocols/template-distribution/Cargo.lock
rm -r ./protocols/v2/subprotocols/template-distribution/target/
rm ./protocols/v2/sv2-ffi/Cargo.lock
rm -r ./protocols/v2/sv2-ffi/target/
