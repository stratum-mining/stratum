#!/bin/sh

# USAGE:
#   ./scripts/check-version-bumps.sh [base-ref]
#
# Fails when a publishable crate has changes relative to <base-ref> (default:
# origin/main) but its version is not above the highest version ever published
# on crates.io. Crates are only published during global releases, so a crate
# modified since its last release must already carry its version bump.

set -eu

cd "$(git rev-parse --show-toplevel)"

BASE="${1:-origin/main}"
# Rename detection is disabled so that a file moved between crates counts
# for the crate it left as well as for the one it joined.
CHANGED=$(git diff --no-renames --name-only "$(git merge-base "$BASE" HEAD)" HEAD)

# Publishable manifests, deepest paths first, so that nested crates
# (derive_codec, stratum-translation) claim their files before the crate
# that contains them does.
MANIFESTS=$(find . -name Cargo.toml -not -path '*/target/*' \
  | xargs grep -l '^\[package\]' | xargs grep -L '^publish = false' \
  | awk '{ print length, $0 }' | sort -rn | cut -d' ' -f2-)

RESPONSE=$(mktemp)
trap 'rm -f "$RESPONSE"' EXIT

FAILED=0
CLAIMED=""
for manifest in $MANIFESTS; do
  dir=$(dirname "$manifest" | sed 's|^\./||')
  files=$(printf '%s\n' "$CHANGED" | grep "^$dir/" || true)
  for claimed in $CLAIMED; do
    files=$(printf '%s\n' "$files" | grep -v "^$claimed/" || true)
  done
  CLAIMED="$CLAIMED $dir"
  [ -n "$files" ] || continue

  name=$(grep -m1 '^name = ' "$manifest" | cut -d'"' -f2)
  current=$(grep -m1 '^version = ' "$manifest" | cut -d'"' -f2)

  # crates.io asks API clients for at most one request per second.
  sleep 1
  status=$(curl -s -o "$RESPONSE" -w '%{http_code}' \
    -A 'stratum-ci (https://github.com/stratum-mining/stratum)' \
    "https://crates.io/api/v1/crates/$name" || true)
  case "$status" in
    200) ;;
    404) echo "$name: not published on crates.io yet, skipping"; continue ;;
    *) echo "::error::crates.io returned HTTP $status for $name"; exit 1 ;;
  esac
  # Yanked versions count too: their numbers can never be published again.
  published=$(jq -r '.versions[].num' "$RESPONSE" | sort -V | tail -n1)

  newest=$(printf '%s\n%s\n' "$published" "$current" | sort -V | tail -n1)
  if [ "$current" = "$published" ] || [ "$newest" != "$current" ]; then
    echo "::error::$name has changes but its version ($current) is not above the latest published on crates.io ($published)"
    FAILED=1
  else
    echo "$name: $published -> $current"
  fi
done
exit $FAILED
