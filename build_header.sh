#! /bin/sh 

touch ./sv2.h
rm ./sv2.h
touch ./sv2.h

cbindgen --crate const_sv2 >> ./sv2.h
cbindgen --crate codec >> ./sv2.h
cbindgen --crate common_messages >> ./sv2.h
cbindgen --crate template_distribution >> ./sv2.h
cbindgen --crate sv2_ffi >> ./sv2.h
