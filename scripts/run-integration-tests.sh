#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SV2_APPS_DIR="$REPO_ROOT/integration-test-framework/sv2-apps"
INTEGRATION_TESTS_DIR="$REPO_ROOT/integration-test-framework/sv2-apps/test/integration-tests"
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
echo "🔧 Adding patch section to override git dependencies..."

# Remove any existing patch section first
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' '/^# Override git dependencies with local paths/,/^$/d' Cargo.toml
else
    sed -i '/^# Override git dependencies with local paths/,/^$/d' Cargo.toml
fi


# Add the patch section at the end of the file
cat >> Cargo.toml << 'EOF'

# Override git dependencies with local paths to avoid version conflicts
# TODO: will need to replace to patch.crates-io as soons as they are available and updated on the sv2-apps repo
[patch."https://github.com/stratum-mining/stratum"]
stratum-common = { path = "../../../../stratum" }
sv1_api = { path = "../../../../sv1" }
key-utils = { path = "../../../../apps-utils/key-utils" }
config_helpers_sv2 = { path = "../../../../apps-utils/config-helpers" }
roles_logic_sv2 = { path = "../../../../sv2/roles-logic-sv2" }
network_helpers_sv2 = { path = "../../../../apps-utils/network-helpers" }
binary_sv2 = { path = "../../../../sv2/binary-sv2" }
binary_codec_sv2 = { path = "../../../../sv2/binary-sv2/codec" }
derive_codec_sv2 = { path = "../../../../sv2/binary-sv2/derive_codec" }
noise_sv2 = { path = "../../../../sv2/noise-sv2" }
framing_sv2 = { path = "../../../../sv2/framing-sv2" }
codec_sv2 = { path = "../../../../sv2/codec-sv2" }
common_messages_sv2 = { path = "../../../../sv2/subprotocols/common-messages" }
template_distribution_sv2 = { path = "../../../../sv2/subprotocols/template-distribution" }
mining_sv2 = { path = "../../../../sv2/subprotocols/mining" }
job_declaration_sv2 = { path = "../../../../sv2/subprotocols/job-declaration" }
channels_sv2 = { path = "../../../../sv2/channels-sv2" }
parsers_sv2 = { path = "../../../../sv2/parsers-sv2" }
buffer_sv2 = { path = "../../../../sv2/buffer-sv2" }
error_handling = { path = "../../../../apps-utils/error-handling" }
rpc_sv2 = { path = "../../../../apps-utils/rpc" }
EOF

echo "✅ Updated Cargo.toml to use local dependencies"
echo "🏃 Running integration tests..."

# Run the integration tests
RUST_BACKTRACE=1 RUST_LOG=debug cargo nextest run --features sv1 --nocapture --verbose

cd "$REPO_ROOT"
echo "✅ Integration tests completed!"
