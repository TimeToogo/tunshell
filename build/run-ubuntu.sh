#!/bin/bash

set -e

BUILD_DIR=$(dirname $0)
BUILD_DIR="`cd "${BUILD_DIR}";pwd`"

docker build -t dmp_build $BUILD_DIR
docker run --rm \
   -v$BUILD_DIR/../:/workspace \
   -v$BUILD_DIR/tmp/build:/tmp/build/ \
   -v$BUILD_DIR/tmp/musl-test:/tmp/musl-test/ \
   -v$BUILD_DIR/tmp/cargo/registry:/root/.cargo/registry \
   dmp_build $@