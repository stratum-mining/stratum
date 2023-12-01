#!/bin/sh

case "$RUN_COMMAND" in
  "pool")
    /stratum/target/release/pool_sv2 -c /conf/pool-config.toml
    ;;
  "jd-server")
    /stratum/target/release/jd_server -c /conf/jds-config.toml
    ;;
  "jd-client")
    /stratum/target/release/jd_server -c /conf/jdc-config.toml
    ;;
  "translator")
    /stratum/target/release/jd_server -c /conf/proxy-config.toml
    ;;
  *)
    echo "Invalid RUN_COMMAND specified"
    exit 1
    ;;
esac
