#!/bin/sh

tarpaulin()
{
  cargo +nightly tarpaulin --verbose
}

cd protocols
tarpaulin
cd ../roles
tarpaulin
cd ../utils
tarpaulin
