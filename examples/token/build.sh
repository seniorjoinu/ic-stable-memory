#!/bin/bash

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")
cd "$SCRIPTPATH" || exit

cargo build --target wasm32-unknown-unknown --package stable-token --release && \
     ic-cdk-optimizer ./target/wasm32-unknown-unknown/release/stable_token.wasm -o ./target/wasm32-unknown-unknown/release/stable-token-opt.wasm