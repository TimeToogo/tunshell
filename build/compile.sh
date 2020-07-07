#!/bin/bash

set -e

if [[ -f $HOME/.cargo/env ]]; then
   source $HOME/.cargo/env
fi

SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`
TARGETS=$(cat targets.txt)

mkdir -p $SCRIPT_DIR/artifacts

echo "$TARGETS" | while IFS=',' read -r TARGET
do
   echo "Building $TARGET..."

   echo "Compiling tunshell-client for $TARGET..."
   cd $SCRIPT_DIR/../
   cross build --release --target $TARGET
   cp $SCRIPT_DIR/../target/$TARGET/release/client $SCRIPT_DIR/artifacts/client-$TARGET
done