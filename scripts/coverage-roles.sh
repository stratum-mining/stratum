#!/bin/bash
tarpaulin() {
  crate_name=$1
  output_dir="target/tarpaulin-reports/$crate_name"
  mkdir -p "$output_dir"
  cargo +nightly tarpaulin --verbose --out Xml --output-dir "$output_dir" --all-features
}

cd roles
tarpaulin

crates=(
  "pool"
  "test-utils/mining-device"
  "test-utils/mining-device-sv1"
  "translator"
  "jd-client"
  "jd-server"
)

for crate in "${crates[@]}"; do
  echo "Running Tarpaulin for $crate..."
  crate_name=$(basename "$crate") 
  cd "$crate" || exit 1            
  tarpaulin "$crate_name-coverage"
  cd - || exit 1
done
