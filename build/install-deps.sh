#!/bin/bash

set -e

TEMPDIR=${TEMPDIR:="$(dirname $0)/tmp"}
cd $TEMPDIR


echo "Installing compile toolchain..."
case "$OSTYPE" in
  msys*)    
    choco install rust activeperl nasm
    echo '##[add-path]%USERPROFILE%\.cargo\bin'
    echo '##[add-path]C:\Perl64'
    ;;

  darwin*)    
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
    ;;
    
  *)
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

    sudo apt update -y
    sudo apt install -y \
        curl \
        wget \
        vim \
        jq \
        binutils \
        make \
        build-essential \
        gcc-arm-linux-gnueabihf \
        crossbuild-essential-armhf
    ;;
esac

echo "Downloading OpenSSL..."
curl -sSf -o openssl.tar.gz https://www.openssl.org/source/openssl-1.1.1.tar.gz
mkdir openssl
tar xzf openssl.tar.gz -C openssl --strip-components 1

echo "Downloading Libsodium..."
curl -sSf -o libsodium.tar.gz https://download.libsodium.org/libsodium/releases/libsodium-1.0.18-stable.tar.gz
mkdir libsodium
tar xzf libsodium.tar.gz -C libsodium --strip-components 1