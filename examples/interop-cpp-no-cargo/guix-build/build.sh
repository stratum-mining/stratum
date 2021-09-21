#! /bin/sh

guix environment \
        -m ./guix-build/example.scm \
        --container gcc-toolchain \
        --pure \
        --no-cwd \
        --share=../interop-cpp/template-provider/=/cpp \
        --share=../../protocols/v2=/rust \
        --share=./=/build \
        -- /build/guix-build/in_container_build.sh
