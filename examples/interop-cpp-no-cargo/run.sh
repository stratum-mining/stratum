#! /bin/sh

touch libsv2_ffi.a
touch a.out

# CLEAN
rm -f libsv2_ffi.a
rm -f a.out
rm -f sv2.h

./rust-build-script.sh ../../protocols/v2/ ../../utils/

g++ -I ../../protocols/v2/sv2-ffi ../interop-cpp/template-provider/template-provider.cpp  libsv2_ffi.a  -lpthread -ldl

./a.out &
provider_pid=$!
sleep 1 # wait for provider to start listening

cargo run --manifest-path ../interop-cpp/Cargo.toml &
run_pid=$!

# If there is a first argument sleep for that long
if [ -n "$1" ]; then
    sleep "$1"

  if ps -p $provider_pid > /dev/null && ps -p $run_pid > /dev/null
  then
      echo "Success!"
      kill $provider_pid
      kill $run_pid
  else
      echo "Failure!!!"
      exit 1
  fi
fi
