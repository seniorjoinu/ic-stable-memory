#!/bin/bash

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")
cd "$SCRIPTPATH" || exit

cargo build --target wasm32-unknown-unknown --package performance-counter --release && \
     ic-cdk-optimizer ./target/wasm32-unknown-unknown/release/performance_counter.wasm -o ./target/wasm32-unknown-unknown/release/performance-counter-opt.wasm