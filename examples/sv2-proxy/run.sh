#! /bin/sh
cargo run --bin test-pool &
pool_pid=$!
sleep 2
cargo run --bin mining-proxy &
proxy_pid=$!
sleep 3
cargo run --bin mining-device &
device_pid=$!
sleep 10
kill $pool_pid
kill $proxy_pid
kill $device_pid
