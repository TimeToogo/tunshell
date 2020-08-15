#!/bin/bash

set -e

if [[ ! -x "$(command -v rustc)" ]];
then
    echo "Installing rust"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
fi

export TUNSHELL_API_PORT="3000"
export TUNSHELL_RELAY_TLS_PORT="3001"
export SQLITE_DB_PATH="$PWD/db.sqlite"
export TLS_RELAY_PRIVATE_KEY="$PWD/certs/development.key"
export TLS_RELAY_CERT="$PWD/certs/development.cert"
export STATIC_DIR="$PWD/static"

if [[ -z "$SKIP_TESTS" ]];
then
    cargo test $@
fi