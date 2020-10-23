#!/bin/bash

set -e

TARGET=$1

if [[ -z "$TARGET" ]]; then
   echo "usage: compile.sh [target]"
   exit 1
fi

if [[ -f $HOME/.cargo/env ]]; then
   source $HOME/.cargo/env
fi


SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`
OUTPUT_PATH="$SCRIPT_DIR/artifacts/client-$TARGET"

mkdir -p $SCRIPT_DIR/artifacts

echo "Compiling tunshell-client for $TARGET..."
cd $SCRIPT_DIR/../../
rustup target add $TARGET

if [[ ! -z "$RUN_TESTS" ]];
then
   cross test -p tunshell-client --target $TARGET
fi

cross build -p tunshell-client --release --target $TARGET
cp $SCRIPT_DIR/../../target/$TARGET/release/client $OUTPUT_PATH

if [[ -x "$(command -v strip)" ]];
then
   echo "Stripping binary..."
   strip $OUTPUT_PATH
fi