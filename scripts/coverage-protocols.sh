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
  "sv1"
  "sv2/binary-sv2/derive_codec"
  "sv2/binary-sv2"
  "sv2/buffer-sv2"
  "sv2/channels-sv2"
  "sv2/noise-sv2"
  "sv2/framing-sv2"
  "sv2/codec-sv2"
  "sv2/extensions-sv2"
  "sv2/subprotocols/common-messages"
  "sv2/subprotocols/template-distribution"
  "sv2/subprotocols/mining"
  "sv2/subprotocols/job-declaration"
  "sv2/parsers-sv2"
  "sv2/handlers-sv2"
  "stratum-core/stratum-translation"
)

for crate in "${crates[@]}"; do
  echo "Running Tarpaulin for $crate..."
  crate_name=$(basename "$crate") 
  cd "$crate" || exit 1
  tarpaulin "$crate_name-coverage"
  cd - || exit 1
done
