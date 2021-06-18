#! /bin/sh

# CLEAN
rm libsv2_ffi.a
rm a.out
rm sv2.h

cargo build --release -p sv2_ffi && cp ../../target/release/libsv2_ffi.a ./
../../build_header.sh

g++ -I ./ ./template-provider/template-provider.cpp  libsv2_ffi.a  -lpthread -ldl

cargo run &
./a.out
