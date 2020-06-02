#!/bin/bash

set -e

TEMPDIR=${TEMPDIR:="$(dirname $0)/tmp"}
cd $TEMPDIR

echo "Installing OpenSSL..."
wget https://www.openssl.org/source/openssl-1.1.1.tar.gz
mkdir openssl
tar xzf openssl-1.1.1.tar.gz -C openssl --strip-components 1

echo "Installing Libsodium..."
wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.18-stable.tar.gz
mkdir libsodium
tar xzf libsodium-1.0.18-stable.tar.gz -C libsodium --strip-components 1