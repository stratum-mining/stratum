#!/bin/sh

case "$RUN_COMMAND" in
  "pool")
    /stratum/target/release/pool_sv2 -c /config/pool-config.toml
    ;;
  "jd-server")
    /stratum/target/release/jd_server -c /config/jds-config.toml
    ;;
  "jd-client")
    /stratum/target/release/jd_server -c /config/jdc-config.toml
    ;;
  "translator")
    /stratum/target/release/jd_server -c /config/proxy-config.toml
    ;;
  *)
    echo "Invalid RUN_COMMAND specified"
    exit 1
    ;;
esac
