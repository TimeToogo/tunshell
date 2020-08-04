#!/bin/bash
export TUNSHELL_API_PORT="3000"
export TUNSHELL_RELAY_TLS_PORT="3001"
export MONGO_CONNECTION_STRING="mongodb://relay:password@localhost:27017/relay"
export TLS_RELAY_PRIVATE_KEY="$PWD/certs/development.key"
export TLS_RELAY_CERT="$PWD/certs/development.cert"
export STATIC_DIR="$PWD/static"

docker-compose start mongo
cargo test $@
