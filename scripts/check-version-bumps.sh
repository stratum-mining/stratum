#!/bin/sh

# USAGE:
#   ./scripts/check-version-bumps.sh [base-ref]
#
# Fails when a publishable crate has changes relative to <base-ref> (default:
# origin/main) but its version is not above the highest version ever published
# on crates.io. Crates are only published during global releases, so a crate
# modified since its last release must already carry its version bump.
#
# When <base-ref> is main, the versions carried by the release tags count as
# published too, so the tags have to be fetched for a local run to agree with
# CI.

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

# On main, a version is also taken once a release tag carries it, on crates.io
# or not: a release candidate holds the versions its release is going to
# publish, so a crate that changes after the candidate was cut belongs to the
# following release and needs a version of its own. A release branch is checked
# against crates.io alone, since what its own candidates carry is what it is
# about to publish.
case "$BASE" in
  main | */main) RELEASE_TAGS=$(git tag --list 'v[0-9]*') ;;
  *) RELEASE_TAGS="" ;;
esac

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
    # Yanked versions count too: their numbers can never be published again.
    200) published=$(jq -r '.versions[].num' "$RESPONSE" | sort -V | tail -n1) ;;
    404) published="" ;;
    *) echo "::error::crates.io returned HTTP $status for $name"; exit 1 ;;
  esac
  tagged=$(for tag in $RELEASE_TAGS; do
    git show "$tag:${manifest#./}" 2>/dev/null | grep -m1 '^version = ' | cut -d'"' -f2
  done | sort -V | tail -n1)

  taken=$(printf '%s\n%s\n' "$published" "$tagged" | sort -V | tail -n1)
  if [ -z "$taken" ]; then
    echo "$name: not released yet, skipping"
    continue
  fi
  where="published on crates.io"
  [ "$taken" = "$published" ] || where="carried by a release tag"

  newest=$(printf '%s\n%s\n' "$taken" "$current" | sort -V | tail -n1)
  if [ "$current" = "$taken" ] || [ "$newest" != "$current" ]; then
    echo "::error::$name has changes but its version ($current) is not above the latest $where ($taken)"
    FAILED=1
  else
    echo "$name: $taken -> $current"
  fi
done
exit $FAILED
