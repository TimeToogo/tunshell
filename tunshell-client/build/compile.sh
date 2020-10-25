#!/bin/bash

set -e

TARGET=$1
CLIENT_NAME=${2:-"$TARGET"}

if [[ -z "$TARGET" ]]; then
   echo "usage: compile.sh [target] [client-name: default to target]"
   exit 1
fi

if [[ -f $HOME/.cargo/env ]]; then
   source $HOME/.cargo/env
fi


SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`
OUTPUT_PATH="$SCRIPT_DIR/artifacts/client-$CLIENT_NAME"

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

if [[ $TARGET =~ "linux" ]]; 
then
   case $TARGET in
      x86_64-unknown-linux-musl|i686-unknown-linux-musl|i586-unknown-linux-musl)
         STRIP="strip"
         ;;
      aarch64-unknown-linux-musl)
         STRIP="aarch64-linux-musl-strip"
         ;;
      arm-unknown-linux-musleabi)
         STRIP="arm-linux-musleabi-strip"
         ;;
      armv7-unknown-linux-musleabihf)
         STRIP="arm-linux-musleabihf-strip"
         ;;
      arm-linux-androideabi)
         STRIP="arm-linux-androideabi-strip"
         ;;
      aarch64-linux-android)
         STRIP="aarch64-linux-android-strip"
         ;;
      *)   
         echo "Unknown linux target: $TARGET"
         exit 1
         ;;
   esac

   echo "Stripping binary using cross docker image ($STRIP)..."
   docker run --rm -v$SCRIPT_DIR:/app/ rustembedded/cross:$TARGET $STRIP /app/artifacts/client-$CLIENT_NAME
elif [[ -x "$(command -v strip)" && $TARGET =~ "apple" ]];
then
   echo "Stripping binary..."
   strip $OUTPUT_PATH
fi