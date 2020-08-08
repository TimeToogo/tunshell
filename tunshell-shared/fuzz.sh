#!/bin/bash

set -e

export CARGO_TARGET_DIR="$PWD/fuzz/target"

cargo afl build
cargo afl fuzz -i fuzz/corpus -o fuzz/out $CARGO_TARGET_DIR/debug/fuzzing
