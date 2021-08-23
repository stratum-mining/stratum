set -e
cargo install cbindgen --force bts
cbindgen -V

SHA1_1=$(sha1sum sv2.h)

BUILD_SCRIPT="./build_header.sh"
sh ./"$BUILD_SCRIPT"

SHA1_2=$(sha1sum sv2.h)

[ "$SHA1_1" = "$SHA1_2" ]; echo "$?"
