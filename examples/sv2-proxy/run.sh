#! /bin/sh
cargo run --bin pool &
pool_pid=$!
sleep 2
cargo run --bin mining-proxy &
proxy_pid=$!
sleep 2
cargo run --bin mining-device &
device_pid=$!
sleep 20
kill $pool_pid
kill $proxy_pid
kill $device_pid
