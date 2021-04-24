#!/bin/bash

set -e

TARGET=$1
OUTPUT_PATH=$2

if [[ -z "$TARGET" ]] || [[ -z "$OUTPUT_PATH" ]]; then
   echo "usage: compile.sh [target] [output path]"
   exit 1
fi

if [[ -f $HOME/.cargo/env ]]; then
   source $HOME/.cargo/env
fi


SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`

mkdir -p $(dirname $OUTPUT_PATH)

echo "Compiling tunshell-client for $TARGET..."
cd $SCRIPT_DIR/../../
rustup target add $TARGET

if [[ ! -z "$RUN_TESTS" ]];
then
   cross test -p tunshell-client --target $TARGET
fi

cross build -p tunshell-client --release --target $TARGET
cp $SCRIPT_DIR/../../target/$TARGET/release/client $OUTPUT_PATH

if [[ $TARGET =~ "linux" ]] || [[ $TARGET =~ "freebsd" ]];
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
      mips-unknown-linux-musl)
         STRIP="mips-linux-muslsf-strip"
         ;;
      x86_64-unknown-freebsd)
         STRIP="x86_64-unknown-freebsd12-strip"
         ;;
      *)   
         echo "Unknown linux target: $TARGET"
         exit 1
         ;;
   esac

   echo "Stripping binary using cross docker image ($STRIP)..."
   docker run --rm -v$(dirname $OUTPUT_PATH):/app/ rustembedded/cross:$TARGET $STRIP /app/$(basename $OUTPUT_PATH)
elif [[ -x "$(command -v strip)" && $TARGET =~ "apple" ]];
then
   echo "Stripping binary..."
   strip $OUTPUT_PATH
fi

echo "Binary saved to: $OUTPUT_PATH"