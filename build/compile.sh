#!/bin/bash

set -e

TARGETS_FILE=$1

if [[ ! -f "$TARGETS_FILE" ]]; then
   echo "usage: compile.sh [targets file]"
   exit 1
fi

if [[ -f $HOME/.cargo/env ]]; then
   source $HOME/.cargo/env
fi


SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`
TARGETS=$(cat $TARGETS_FILE)

mkdir -p $SCRIPT_DIR/artifacts

echo "$TARGETS" | while IFS=',' read -r TARGET
do
   echo "Building $TARGET..."

   echo "Compiling tunshell-client for $TARGET..."
   cd $SCRIPT_DIR/../
   rustup target add $TARGET
   cross build  --release --target $TARGET
   cp $SCRIPT_DIR/../target/$TARGET/release/client $SCRIPT_DIR/artifacts/client-$TARGET
done