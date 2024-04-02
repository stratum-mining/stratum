#!/bin/bash

# Get the list of paths to `Cargo.toml` files
crates=$(find . -name Cargo.toml -exec dirname {} \; | sort)

# Filter out crates that are not published to crates.io
filter=("benches" "examples" "test" "roles")
for f in "${filter[@]}"; do
    crates=$(echo "$crates" | grep -v "$f")
done

# Loop through each crate, while avoiding root workspace Cargo.toml and files under `target` directory
for crate in $crates; do
    if [ "$crate" != "./protocols" ] && \
       [ "$crate" != "./common" ] && \
       [ "$crate" != "./roles" ] && \
       [ "$crate" != "./utils" ] && \
       ! echo "$crate" | grep -q "target"; then

        cd "$crate"

        # Check if there were any changes between dev and main
        git diff --quiet "dev" "main" -- .
        if [ $? -ne 0 ]; then

            # Check if crate versions on dev and main are identical
            version_dev=$(git show dev:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
            version_main=$(git show main:./Cargo.toml | awk -F' = ' '$1 == "version" {gsub(/[ "]+/, "", $2); print $2}')
            if [ "$version_dev" = "$version_main" ]; then
               echo "Changes detected in crate $crate between dev and main branches! Versions on dev and main branches are identical ($version_dev), so you should bump the crate version on dev before merging into main."
               exit 1
            fi
        fi

        cd - >/dev/null
    fi
done