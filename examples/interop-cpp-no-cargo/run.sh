#! /bin/sh

touch libsv2_ffi.a
touch a.out

# CLEAN
rm -f libsv2_ffi.a
rm -f a.out
rm -f sv2.h

./rust-build-script.sh ../../protocols/v2/

g++ -I ../../protocols/v2/sv2-ffi ../interop-cpp/template-provider/template-provider.cpp  libsv2_ffi.a  -lpthread -ldl

cargo run --manifest-path ../interop-cpp/Cargo.toml &
./a.out
