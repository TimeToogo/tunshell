#!/bin/bash

set -e

# Initialise testing environment
cd ../tunshell-server/
SKIP_TESTS=true . ./test.sh
cd ../tunshell-tests/

export RUSTFLAGS="--cfg integration_test"
export CARGO_TARGET_DIR="$PWD/target"

cargo test $@
