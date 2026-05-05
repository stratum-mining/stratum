#!/bin/sh

# USAGE:
#   ./scripts/cargo-publish.sh <crate-dir>

# the script returns 0 on success of cargo publish, and 1 on failure
# the only exception is when cargo publish fails because the crate is already published
# in that case, the script also returns 0

CRATE_DIR="$1"

echo "Publishing crate in directory: $CRATE_DIR"

cd "$CRATE_DIR"

CARGO_COMMAND="cargo publish"

OUTPUT="$($CARGO_COMMAND 2>&1)"
EXIT_CODE=$?
echo "Ran cargo command, exit code was $EXIT_CODE"

if [ "$EXIT_CODE" -eq 0 ] ; then
  echo "Publish command succeeded: $CRATE_DIR"
  exit 0
fi

# If cargo failed, check whether the crate version was already published.
# Cargo has used both of these error strings across versions:
# - "already uploaded" (rust 1.75.0)
# - "already exists on crates.io index" (rust 1.85.0)
if echo "$OUTPUT" | grep -Eq "already uploaded|already exists on crates.io index"; then
  echo "Crate is already published: $CRATE_DIR"
  exit 0
fi

echo "Publish command failed for $CRATE_DIR"
echo "$OUTPUT"
exit 1
