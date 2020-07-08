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
    choco install rust
    rustup toolchain uninstall stable-x86_64-unknown-linux-gnu
    echo '##[add-path]%USERPROFILE%\.cargo\bin'
    ;;
  
  darwin*)    
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
    ;;
    
  *)
    $SUDO apt update -y
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
    ;;
esac

echo "Installing cross..."
cargo install cross

