#! /bin/sh

touch libsv2_ffi.a
touch a.out

# CLEAN
rm -f libsv2_ffi.a
rm -f a.out
rm -f sv2.h

#./rust-build-script.sh ../../protocols/v2/
./guix-build/build.sh

# 
cargo run --manifest-path ../interop-cpp/Cargo.toml &
./a.out
