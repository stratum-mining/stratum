#!/usr/bin/env bash

set -ex

FEATURES="simple_http simple_tcp simple_uds proxy"

cargo --version
rustc --version

# Some tests require certain toolchain types.
NIGHTLY=false
if cargo --version | grep nightly; then
    NIGHTLY=true
fi

# Pin dependencies as required if we are using MSRV toolchain.
if cargo --version | grep "1\.48"; then
    # 1.0.157 uses syn 2.0 which requires edition 2021
    cargo update -p serde --precise 1.0.156
fi

# Make all cargo invocations verbose
export CARGO_TERM_VERBOSE=true

# Defaults / sanity checks
cargo build
cargo test

if [ "$DO_LINT" = true ]
then
    cargo clippy --all-features --all-targets -- -D warnings
fi

if [ "$DO_FEATURE_MATRIX" = true ]; then
    cargo build --no-default-features
    cargo test --no-default-features

    # All features
    cargo build --no-default-features --features="$FEATURES"
    cargo test --no-default-features --features="$FEATURES"

    # Single features
    for feature in ${FEATURES}
    do
        cargo test --no-default-features --features="$feature"
    done
fi

# Build the docs if told to (this only works with the nightly toolchain)
if [ "$DO_DOCSRS" = true ]; then
    RUSTDOCFLAGS="--cfg docsrs -D warnings -D rustdoc::broken-intra-doc-links" cargo +nightly doc --all-features
fi

# Build the docs with a stable toolchain, in unison with the DO_DOCSRS command
# above this checks that we feature guarded docs imports correctly.
if [ "$DO_DOCS" = true ]; then
    RUSTDOCFLAGS="-D warnings" cargo +stable doc --all-features
fi

# Run formatter if told to.
if [ "$DO_FMT" = true ]; then
    if [ "$NIGHTLY" = false ]; then
        echo "DO_FMT requires a nightly toolchain (consider using RUSTUP_TOOLCHAIN)"
        exit 1
    fi
    rustup component add rustfmt
    cargo fmt --check
fi
