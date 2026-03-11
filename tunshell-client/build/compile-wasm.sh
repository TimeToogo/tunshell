#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR/../"


# Ignore conflicting dependencies which require different
# feature flags based on target. Can be removed once
# https://github.com/rust-lang/cargo/issues/1197 is resolved
BACKUP_FILE="$(mktemp "$PWD/Cargo.toml.orig.XXXXXX")"

cleanup() {
    rm -f Cargo.toml.new
    if [ -f "$BACKUP_FILE" ]; then
        mv "$BACKUP_FILE" Cargo.toml
    fi
}

cp Cargo.toml "$BACKUP_FILE"
trap cleanup EXIT
sed 's/.*\#no-wasm/\#/g' Cargo.toml > Cargo.toml.new
mv Cargo.toml.new Cargo.toml

wasm-pack build
mkdir -p ../website/services/wasm/
cp -vaR pkg/* ../website/services/wasm/
