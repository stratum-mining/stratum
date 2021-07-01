#! /bin/sh

sv2_ffi_dir=/gnu/store/`ls /gnu/store/ | grep rust-sv2` 

g++ \
 -I $sv2_ffi_dir \
  /example/template-provider.cpp \
  $sv2_ffi_dir/libsv2_ffi.a \
 -lpthread \
 -ldl \
 -o /example/example-of-guix-build/a.out

