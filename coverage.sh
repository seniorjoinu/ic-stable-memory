#!/bin/bash

if [[ $1 == "--test" ]]; then
  rm -f *.profraw
  CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
fi
mkdir -p "target/coverage/html"
rm -rf "./target/coverage/html/*"
grcov . --binary-path ./target/debug/deps/ -s . -t html --branch --ignore-not-existing --ignore '../*' --ignore "/*" --ignore "target/debug/build/**/*" --ignore "src/benches/**/*" -o target/coverage/html