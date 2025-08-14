#!/bin/bash
tarpaulin() {
  crate_name=$1
  output_dir="target/tarpaulin-reports/$crate_name"
  mkdir -p "$output_dir"
  cargo +nightly tarpaulin --verbose --out Xml --output-dir "$output_dir"
}

cd protocols
tarpaulin

crates=(
  "v1"
  "v2/binary-sv2/codec"
  "v2/binary-sv2/derive_codec"
  "v2/binary-sv2"
  "v2/channels-sv2"
  "v2/noise-sv2"
  "v2/framing-sv2"
  "v2/codec-sv2"
  "v2/subprotocols/common-messages"
  "v2/subprotocols/template-distribution"
  "v2/subprotocols/mining"
  "v2/subprotocols/job-declaration"
  "v2/roles-logic-sv2"
  "v2/parsers-sv2"
)

for crate in "${crates[@]}"; do
  echo "Running Tarpaulin for $crate..."
  crate_name=$(basename "$crate") 
  cd "$crate" || exit 1            
  tarpaulin "$crate_name-coverage"
  cd - || exit 1
done
