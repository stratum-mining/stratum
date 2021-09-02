#! /bin/sh

cargo package --manifest-path ../../../../protocols/v2/sv2-ffi/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/binary-sv2/binary-sv2/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/binary-sv2/no-serde-sv2/codec/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/binary-sv2/no-serde-sv2/derive_codec/Cargo.toml --allow-dirty 
cargo package --manifest-path ../../../../protocols/v2/framing-sv2/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/const-sv2/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/codec-sv2/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/subprotocols/common-messages/Cargo.toml --allow-dirty
cargo package --manifest-path ../../../../protocols/v2/subprotocols/template-distribution/Cargo.toml --allow-dirty
cargo package --manifest-path ./quickcheck/Cargo.toml --allow-dirty
cargo package --manifest-path ./quickcheck_macros/Cargo.toml --allow-dirty

guix environment \
        -m ./example.scm \
        --container gcc-toolchain \
        --pure \
        --no-cwd \
        --share=../=/example \
        -- /example/in_container_build.sh
