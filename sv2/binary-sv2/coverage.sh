#!/bin/bash
set -e

echo "=== Generating Coverage Report for binary_sv2 ==="

# Create coverage directory in target
COVERAGE_DIR="../../target/coverage/binary_sv2"
mkdir -p "$COVERAGE_DIR"

# Clean previous coverage data
rm -f "$COVERAGE_DIR"/*.profraw "$COVERAGE_DIR"/*.profdata
cargo clean

# Set up environment variables for coverage
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Cinstrument-coverage"
export LLVM_PROFILE_FILE="$COVERAGE_DIR/coverage-%p-%m.profraw"

# Run tests with coverage instrumentation
echo "Running tests with coverage instrumentation..."
cargo test

# Merge coverage data
echo "Merging coverage data..."
llvm-profdata merge -sparse "$COVERAGE_DIR"/coverage-*.profraw -o "$COVERAGE_DIR"/binary_sv2.profdata

# Find the test binary (the one with the most recent timestamp)
TEST_BINARY=$(find ../../target/debug/deps -name 'binary_sv2-*' -type f -executable | head -1)

if [ -z "$TEST_BINARY" ]; then
    echo "Error: Could not find test binary"
    exit 1
fi

echo "Using test binary: $TEST_BINARY"

# Generate text summary
echo ""
echo "=== Coverage Summary ==="
llvm-cov report \
    --instr-profile="$COVERAGE_DIR"/binary_sv2.profdata \
    "$TEST_BINARY" \
    --ignore-filename-regex='/.cargo/registry' \
    --ignore-filename-regex='/rustc/' \
    --ignore-filename-regex='tests/'

# Generate detailed HTML report
echo ""
echo "Generating HTML coverage report..."
llvm-cov show \
    --instr-profile="$COVERAGE_DIR"/binary_sv2.profdata \
    "$TEST_BINARY" \
    --format=html \
    --output-dir="$COVERAGE_DIR"/html \
    --ignore-filename-regex='/.cargo/registry' \
    --ignore-filename-regex='/rustc/' \
    --ignore-filename-regex='tests/' \
    --show-instantiations=false

echo ""
echo "=== Coverage report generated! ==="
echo "Text summary shown above"
echo "HTML report: $COVERAGE_DIR/html/index.html"
echo ""
echo "To view the HTML report, open: file://$COVERAGE_DIR/html/index.html"
