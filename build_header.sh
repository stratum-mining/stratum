#! /bin/sh 

rm -f ./sv2.h
touch ./sv2.h

cbindgen --crate const_sv2 >> ./sv2.h
cbindgen --crate binary_codec_sv2 >> ./sv2.h
cbindgen --crate common_messages_sv2 >> ./sv2.h
cbindgen --crate template_distribution_sv2 >> ./sv2.h
cbindgen --crate sv2_ffi >> ./sv2.h
