if [ ! -d "utils/stratum-message-generator/" ]; then
  cd utils
  git clone https://github.com/stratum-mining/stratum-message-generator
  cd ..
fi

cd utils/stratum-message-generator/
RUST_LOG=debug cargo run ../../test/message-generator/test/pool-sri-test-close-channel/pool-sri-test-close-channel.json || { echo 'mg test failed' ; exit 1; }

sleep 10
