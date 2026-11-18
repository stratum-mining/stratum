#!/bin/sh

# USAGE:
#   ./scripts/check-published-crates.sh
#
# Fails when a publishable crate carries a version that is already on crates.io
# while its contents differ from what was published under that version.
# `cargo publish` refuses a version that already exists, and the release
# workflow takes that refusal as "already released", so such a crate would be
# left out of the release without any error. A crate whose version is not on
# crates.io yet is about to be published, and passes.

set -eu

cd "$(git rev-parse --show-toplevel)"

MANIFESTS=$(find . -name Cargo.toml -not -path '*/target/*' \
  | xargs grep -l '^\[package\]' | xargs grep -L '^publish = false' | sort)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# cargo writes these into every package itself. `Cargo.toml.orig`, which it
# also adds, is the manifest as the tree had it, and is compared.
GENERATED='^(\.cargo_vcs_info\.json|Cargo\.lock|Cargo\.toml)$'

# cargo publishes a dependency by its version requirement and drops where the
# tree fetches it from, so the git source of a dependency is not part of what
# a manifest publishes.
published_form() {
  sed -E -e ':again' \
    -e 's/([{,] *)(git|branch|tag|rev) = "[^"]*",? */\1/' -e 'tagain' \
    -e 's/, *}/ }/' "$1"
}

# The dependency requirements of package $2 in the manifest $1, as cargo reads
# them: what the manifest inherits from the workspace is resolved, which its
# own text does not show. Dev-dependencies are left out, since nothing that
# depends on a published crate ever builds them.
requirements() {
  # On its own line, so that the script stops if cargo cannot read the manifest.
  cargo metadata --no-deps --format-version 1 --manifest-path "$1" > "$WORK/metadata.json"
  jq -S --arg name "$2" '[.packages[] | select(.name == $name)
    | .dependencies[] | select(.kind != "dev")
    | {name, rename, req, kind, optional, target, uses_default_features,
       features: (.features | sort)}] | sort_by(.name, .kind, .target)' "$WORK/metadata.json"
}

FAILED=0
for manifest in $MANIFESTS; do
  dir=$(dirname "$manifest" | sed 's|^\./||')
  name=$(grep -m1 '^name = ' "$manifest" | cut -d'"' -f2)
  version=$(grep -m1 '^version = ' "$manifest" | cut -d'"' -f2)

  # crates.io asks API clients for at most one request per second.
  sleep 1
  status=$(curl -sL -o "$WORK/published.crate" -w '%{http_code}' \
    -A 'stratum-ci (https://github.com/stratum-mining/stratum)' \
    "https://crates.io/api/v1/crates/$name/$version/download" || true)
  case "$status" in
    200) ;;
    404) echo "$name $version: not on crates.io, to be published"; continue ;;
    *) echo "::error::crates.io returned HTTP $status for $name $version"; exit 1 ;;
  esac

  rm -rf "$WORK/published"
  mkdir "$WORK/published"
  tar -xzf "$WORK/published.crate" -C "$WORK/published" --strip-components=1

  # On its own line, so that the script stops if cargo cannot list the package.
  cargo package --list --allow-dirty -p "$name" > "$WORK/package.list"
  grep -Ev "$GENERATED" "$WORK/package.list" | sort > "$WORK/tree.list"
  (cd "$WORK/published" && find . -type f | sed 's|^\./||') \
    | grep -Ev "$GENERATED" | sort > "$WORK/published.list"

  published_form "$dir/Cargo.toml" > "$WORK/tree.toml"
  published_form "$WORK/published/Cargo.toml.orig" > "$WORK/published.toml"
  requirements Cargo.toml "$name" > "$WORK/tree.requirements"
  requirements "$WORK/published/Cargo.toml" "$name" > "$WORK/published.requirements"

  # Files only one side has, then files both have with different contents.
  changed=$(
    comm -3 "$WORK/tree.list" "$WORK/published.list" | tr -d '\t'
    comm -12 "$WORK/tree.list" "$WORK/published.list" | while read -r file; do
      if [ "$file" != Cargo.toml.orig ]; then
        cmp -s "$dir/$file" "$WORK/published/$file" || echo "$file"
      elif ! cmp -s "$WORK/tree.toml" "$WORK/published.toml"; then
        echo Cargo.toml
      elif ! cmp -s "$WORK/tree.requirements" "$WORK/published.requirements"; then
        echo "Cargo.toml(workspace-dependencies)"
      fi
    done
  )

  if [ -n "$changed" ]; then
    # Unquoted on purpose: the file names go on the error's own line.
    echo "::error::$name $version is on crates.io with different contents, so it needs a version bump:" $changed
    FAILED=1
  else
    echo "$name $version: on crates.io, identical"
  fi
done
exit $FAILED
