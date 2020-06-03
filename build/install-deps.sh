#!/bin/bash

set -e



TEMPDIR=${TEMPDIR:="$(dirname $0)/tmp"}
cd $TEMPDIR

echo "Installing rust..."
case "$OSTYPE" in
  msys*)    
    curl https://static.rust-lang.org/rustup/dist/i686-pc-windows-gnu/rustup-init.exe -o rust-init.exe
    ./rust-init.exe 
    ;;
  *)
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
    ;;
esac
source $HOME/.cargo/env

echo "Installing OpenSSL..."
wget https://www.openssl.org/source/openssl-1.1.1.tar.gz
mkdir openssl
tar xzf openssl-1.1.1.tar.gz -C openssl --strip-components 1

echo "Installing Libsodium..."
wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.18-stable.tar.gz
mkdir libsodium
tar xzf libsodium-1.0.18-stable.tar.gz -C libsodium --strip-components 1