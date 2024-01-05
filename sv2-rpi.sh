#!/bin/sh

# This program utilizes cargo-cross to automate the generation of a tarball package distribution of
# SRI releases targeting the Rasperry Pi OS (32 and 64 bits).

print_help() {
  echo "StratumV2 Reference Implementation (SRI) distribution for Raspberry Pi OS (32 and 64 bits)"
  echo ""
  echo "Usage:"
  echo "$ ./sv2-rpi.sh <role> <bits>"
  echo ""
  echo "Available options for <role> are:"
  echo "jd_client"
  echo "jd_server"
  echo "mining_device"
  echo "mining_proxy_sv2"
  echo "pool_sv2"
  echo "translator_v2"
  echo ""
  echo "Available options for <bits> are:"
  echo "32"
  echo "64"
  echo ""
  echo "For example:"
  echo "$ ./sv2-rpi.sh jd_server 64"
}

build(){
  cross build -p $PKG --target $TARGET --release
}

clean_pkg(){
  rm -rf pkg
}

clean_tar(){
  rm -rf tar/$TARBALL
}

pkg(){
  clean_pkg
  mkdir -p pkg/$PKG
  mkdir -p pkg/$PKG/bin
  mkdir -p pkg/$PKG/etc/sri
  mkdir -p tar
}

tarball(){
  TARBALL=$PKG-$TARGET.tar.gz
  clean_tar
  pushd pkg
  tar -cvf ../tar/$TARBALL .
  popd
  echo "tar/$TARBALL created with success."
}

jd_client(){
  build
  pkg

  install target/$TARGET/release/jd_client pkg/$PKG/bin
  # install roles/jd-client/config-examples/jdc-config-hosted-example.toml pkg/$PKG/etc/sri
  # install roles/jd-client/config-examples/jdc-config-local-example.toml pkg/$PKG/etc/sri

  echo "finished bootstrapping package $PKG... creating tarball now..."
  tarball
}

jd_server(){
  build
  pkg

  install target/$TARGET/release/jd_server pkg/$PKG/bin
  install roles/jd-server/config-examples/jds-config-hosted-example.toml pkg/$PKG/etc/sri
  install roles/jd-server/config-examples/jds-config-local-example.toml pkg/$PKG/etc/sri

  echo "finished bootstrapping package $PKG... creating tarball now..."
  tarball
}

mining_proxy_sv2(){
  build
  pkg

  install target/$TARGET/release/mining_proxy_sv2 pkg/$PKG/bin

  echo "finished bootstrapping package $PKG... creating tarball now..."
  tarball
}

pool_sv2(){
  build
  pkg

  install target/$TARGET/release/pool_sv2 pkg/$PKG/bin
  install roles/pool/config-examples/pool-config-hosted-tp-example.toml pkg/$PKG/etc/sri
  install roles/pool/config-examples/pool-config-local-tp-example.toml pkg/$PKG/etc/sri

  echo "finished bootstrapping package $PKG... creating tarball now..."
  tarball
}

translator_sv2(){
  build
  pkg

  install target/$TARGET/release/translator_sv2 pkg/$PKG/bin
#  install roles/translator/config-examples/translator-config-local-jdc-example.toml pkg/$PKG/etc/sri
#  install roles/translator/config-examples/translator-config-local-pool-example.toml pkg/$PKG/etc/sri

  echo "finished bootstrapping package $PKG... creating tarball now..."
  tarball
}

CMD=$1
BITS=$2

case $CMD in
  "" | "h" | "-h" | "help" | "--help")
    print_help
    exit 0
    ;;
  *)
  ;;
esac

case $BITS in
  64*)     ARCH=aarch64;TARGET=$ARCH-unknown-linux-gnu;;
  32*)     ARCH=arm;TARGET=$ARCH-unknown-linux-gnueabi;;
  *)       echo "Error: must choose 32 or 64"; exit 1
  ;;
esac

case $CMD in
  *)
    shift
    PKG=$CMD
    ${PKG} $@
    if [ $? = 127 ]; then
      echo "Error: '$PKG' is not a known SRI package." >&2
      echo "Run './sv2-rpi.sh --help' for a list of known subcommands." >&2
        exit 1
    fi
  ;;
esac
