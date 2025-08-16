#! /usr/bin/sh

# This program utilizes cargo-smart-release to auto-version and publish the sv2 crates
# See https://crates.io/crates/cargo-smart-release for more information about the crate

# *** For publishing the following flags should be passed as arguments:
# `-e -u --no-push`

# *** For updating versions without publishing the following flags should be passed as arguments:
# `-e -u --no-publish --no-push --no-changelog`

output=$(cargo smart-release \
    sv1_api \
    binary_sv2 \
    binary_codec_sv2 \
    derive_codec_sv2 \
    codec_sv2 \
    framing_sv2 \
    noise_sv2 \
    roles_logic_sv2 \
    common_messages_sv2 \
    job_declaration_sv2 \
    mining_sv2 \
    template_distribution_sv2 \
    buffer_sv2 \
    error_handling \
    network_helpers \
    translator_sv2 \
    pool_sv2 \
    mining_proxy_sv2 \
    $@)

if echo "$output" | grep -q "Error: There is no crate eligible for publishing"; then
    echo "No crate eligible for publishing. Exiting with success code."
    exit 0
fi
echo "$output"
