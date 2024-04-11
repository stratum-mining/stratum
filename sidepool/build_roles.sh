#!/bin/bash

# Define the root directory of the cargo workspace relative to the script location
ROOT_DIR=$(dirname "$0")/../roles

# Navigate to the root directory of the cargo workspace
cd "$ROOT_DIR" || exit

echo "Current directory set to $(pwd), starting build process..."

# Build each Cargo project
echo "Building Rust projects..."
cargo build --release

# List of projects to build and their target directories for binaries
declare -a projects=("mining_proxy_sv2" "pool_sv2" "translator_sv2" "jd_client" "jd_server")

# Copy the compiled binaries to a designated directory for Docker to use
for project in "${projects[@]}"; do
    echo "Preparing $project for Docker..."
    # Ensure the target directory exists
    mkdir -p ../sidepool/release/
    # Copy the binary    
    cp ../roles/target/release/$project ../sidepool/release/
done

echo "All projects built and binaries are ready for Docker."
