#! /bin/sh

# Build rust library
/build/rust-build-script.sh /rust

g++ \
 -I /rust/sv2-ffi/ \
  /cpp/template-provider.cpp \
  ./libsv2_ffi.a \
 -lpthread \
 -ldl \
 -o /build/a.out
