#!/bin/bash

set -e

SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`

cd $SCRIPT_DIR/../

wasm-pack build
cp -aR pkg/* ../website/services/wasm/