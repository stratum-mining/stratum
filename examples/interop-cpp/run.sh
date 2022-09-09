#! /bin/sh

touch libsv2_ffi.a
touch a.out

# CLEAN
rm -f libsv2_ffi.a
rm -f a.out
rm -f sv2.h

cargo build --release -p sv2_ffi && cp ../../target/release/libsv2_ffi.a ./
../../build_header.sh

g++ -I ./ ./template-provider/template-provider.cpp  libsv2_ffi.a  -lpthread -ldl

cargo run &
timeout 30 ./a.out
