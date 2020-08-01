#!/bin/bash

set -e

SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`

cd $SCRIPT_DIR/../


# Ignore conflicting dependencies which require different
# feature flags based on target. Can be removed once 
# https://github.com/rust-lang/cargo/issues/1197 is resolved
cp Cargo.toml Cargo.toml.orig
trap "mv $PWD/Cargo.toml.orig $PWD/Cargo.toml" EXIT
cat Cargo.toml | sed 's/.*\#no-wasm/\#/g' > Cargo.toml.new
mv Cargo.toml.new Cargo.toml

wasm-pack build
mkdir -p ../website/services/wasm/
cp -aR pkg/* ../website/services/wasm/