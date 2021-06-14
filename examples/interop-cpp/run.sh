#! /bin/sh

cargo build --release -p sv2_ffi

g++ -I ./ ./template-provider/template-provider.cpp  libsv2_ffi.a  -lpthread -ldl

cargo run &
./a.out
