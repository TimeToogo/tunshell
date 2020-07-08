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

mkdir -p $SCRIPT_DIR/artifacts

echo "Building $TARGET..."

echo "Compiling tunshell-client for $TARGET..."
cd $SCRIPT_DIR/../
rustup target add $TARGET
cross build --release --target $TARGET
cp $SCRIPT_DIR/../target/$TARGET/release/client $SCRIPT_DIR/artifacts/client-$TARGET