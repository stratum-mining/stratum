#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SV2_APPS_DIR="$REPO_ROOT/integration-test-framework/sv2-apps"
INTEGRATION_TESTS_DIR="$REPO_ROOT/integration-test-framework/sv2-apps/integration-tests"
SV2_APPS_REPO_URL=https://github.com/stratum-mining/sv2-apps.git

echo "🧪 Running integration tests for sv2-miner-apps changes..."
echo "📁 Repository root: $REPO_ROOT"
echo "📁 Integration test dir: $INTEGRATION_TESTS_DIR"
mkdir -p "$REPO_ROOT/integration-test-framework"

# Clone/update integration test framework
if [ ! -d "$SV2_APPS_DIR" ]; then
    echo "📥 Cloning integration test framework..."
    cd "$(dirname "$SV2_APPS_DIR")"
    git clone $SV2_APPS_REPO_URL
else
    echo "🔄 Updating integration test framework..."
    cd "$SV2_APPS_DIR"
    git fetch origin
    git reset --hard origin/main
fi

if cargo nextest --version &>/dev/null; then
    echo "✅ cargo-nextest is already installed."
else
    echo "🔧 Configuring cargo nextest..."
    curl -L --proto '=https' --tlsv1.2 -sSf \
        https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

    cargo binstall cargo-nextest --secure --no-confirm
    echo "✅ cargo-nextest installed successfully."
fi

cd "$INTEGRATION_TESTS_DIR"

# # Add patch section to override all git dependencies with local paths
echo "🔧 Adding patch section to override dependencies..."

# Remove any existing patch section first
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' '/^# Override dependencies with local paths/,/^$/d' Cargo.toml
else
    sed -i '/^# Override dependencies with local paths/,/^$/d' Cargo.toml
fi


# Add the patch section at the end of the file
cat >> Cargo.toml << 'EOF'

# Override dependencies with local paths
[patch.crates-io] 
stratum-core = {path = "../../../stratum-core"}

[patch."https://github.com/stratum-mining/stratum"]
stratum-core = {path = "../../../stratum-core"}
EOF

echo "✅ Updated Cargo.toml to use local dependencies"
echo "🏃 Running integration tests..."

# Run the integration tests
RUST_BACKTRACE=1 RUST_LOG=debug cargo nextest run --nocapture --verbose

cd "$REPO_ROOT"
echo "✅ Integration tests completed!"
