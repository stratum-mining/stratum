#!/bin/bash
set -euo pipefail

print_help() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  -p, --pr NUMBER       Specify a companion PR number.
                        If your PR must be in sync with another PR in the
                        sv2-apps repo, pass that PR number here.
  -h, --help            Show this help message.

Behavior:
  If the PR number is not provided:
    1. Tries to read COMPANION_PR_NUMBER from the environment.
    2. If still not defined, defaults to "main".
EOF
}

PR_NUMBER=""

# ---- parse args ----
while [[ $# -gt 0 ]]; do
    case "$1" in
        -p|--pr)
            if [[ -z "${2:-}" ]]; then
                echo "❌ Error: Missing value for $1" >&2
                exit 1
            fi
            PR_NUMBER="$2"
            shift 2
            ;;
        -h|--help)
            print_help
            exit 0
            ;;
        *)
            echo "❌ Unknown option: $1" >&2
            print_help
            exit 1
            ;;
    esac
done

# ---- fallback logic ----
if [[ -n "$PR_NUMBER" ]]; then
    # user passed --pr, validate it
    if ! [[ "$PR_NUMBER" =~ ^[0-9]+$ ]]; then
        echo "❌ Error: PR number must be numeric. Got: '$PR_NUMBER'" >&2
        exit 1
    fi
elif [[ -n "${COMPANION_PR_NUMBER:-}" ]]; then
    # env var exists - use it
    if ! [[ "$COMPANION_PR_NUMBER" =~ ^[0-9]+$ ]]; then
        echo "❌ Error: COMPANION_PR_NUMBER must be numeric. Got: '$COMPANION_PR_NUMBER'" >&2
        exit 1
    fi
    PR_NUMBER="$COMPANION_PR_NUMBER"
else
    # fallback to main
    PR_NUMBER="main"
fi

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
    cd $SV2_APPS_DIR
else
    cd "$SV2_APPS_DIR"
fi

# At this point, we're inside $SV2_APPS_DIR
# Handle PR or main
if [ "$PR_NUMBER" = "main" ]; then
    echo "🌱 Using main branch"
    git checkout main
    git reset --hard origin/main
else
    echo "🔍 Fetching PR #$PR_NUMBER"
    git fetch origin pull/"$PR_NUMBER"/head:pr-"$PR_NUMBER" --force --update-head-ok
    git checkout pr-"$PR_NUMBER"
    git clean -fdx
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

# Cargo only honours `[patch]` in the workspace root manifest and silently
# ignores it anywhere else, so the patch goes wherever cargo says the root of
# the workspace holding the integration tests is.
WORKSPACE_MANIFEST="$(cargo locate-project --workspace --message-format plain)"
STRATUM_CORE_DIR="$REPO_ROOT/stratum-core"

# # Add patch section to override all git dependencies with local paths
echo "🔧 Adding patch section to override dependencies in $WORKSPACE_MANIFEST..."

# Remove any existing patch section first
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' '/^# Override dependencies with local paths/,/^$/d' "$WORKSPACE_MANIFEST"
else
    sed -i '/^# Override dependencies with local paths/,/^$/d' "$WORKSPACE_MANIFEST"
fi


# Add the patch section at the end of the file. The path is absolute so that it
# does not depend on where the workspace root sits inside sv2-apps.
cat >> "$WORKSPACE_MANIFEST" << EOF
# Override dependencies with local paths
[patch.crates-io]
stratum-core = {path = "$STRATUM_CORE_DIR"}
[patch."https://github.com/stratum-mining/stratum"]
stratum-core = {path = "$STRATUM_CORE_DIR"}
EOF

# Force a refresh of Cargo.lock so the patched stratum-core is not ignored after
# a crate version change. When stratum-core's version changes, the lockfile may still
# pin an older version, causing Cargo to ignore the patch entirely
# ("patch was not used in the crate graph"). Running `cargo update -p stratum-core`
# updates the lockfile to match the patched crate, ensuring the local override
# actually takes effect.
cargo update -p stratum-core

# An ignored patch would leave the tests running against the published
# stratum-core instead of this checkout. Path dependencies are the only ones
# cargo reports with a filesystem location, so anything else means the override
# is not in effect.
if ! cargo tree -i stratum-core --depth 0 | grep -Eq '^stratum-core v[^ ]+ \(/'; then
    echo "❌ Error: stratum-core is not resolved from $STRATUM_CORE_DIR" >&2
    cargo tree -i stratum-core --depth 0 >&2 || true
    exit 1
fi

echo "✅ Updated Cargo.toml to use local dependencies"
echo "🏃 Running integration tests..."

# Failure triage: re-run a single test with captured logs shown live:
#   RUST_LOG=debug cargo nextest run \
#     -E 'test(<name>)' --no-capture
RUST_BACKTRACE=1 RUST_LOG=debug cargo nextest run

cd "$REPO_ROOT"
echo "✅ Integration tests completed!"
