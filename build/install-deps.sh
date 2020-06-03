#!/bin/bash

set -e

TEMPDIR=${TEMPDIR:="$(dirname $0)/tmp"}
cd $TEMPDIR

SUDO="sudo"

if [[ ! -x "$(command -v sudo)" ]]; then
 SUDO=""
fi

echo "Installing compile toolchain..."
case "$OSTYPE" in
  msys*)    
    choco install rust msys2 nasm
    echo '##[add-path]%USERPROFILE%\.cargo\bin'
    echo '##[add-path]C:\Perl64\bin'
    ;;

  darwin*)    
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
    ;;
    
  *)
    $SUDO apt update -y
    $SUDO apt install -y \
        curl \
        wget \
        vim \
        jq \
        git \
        binutils \
        pkgconf \
        make \
        build-essential \
        musl-dev \
        musl-tools
    
    ln -s /usr/include/x86_64-linux-gnu/asm /usr/include/x86_64-linux-musl/asm && \
    ln -s /usr/include/asm-generic /usr/include/x86_64-linux-musl/asm-generic && \
    ln -s /usr/include/linux /usr/include/x86_64-linux-musl/linux

    git clone https://github.com/richfelker/musl-cross-make.git

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
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