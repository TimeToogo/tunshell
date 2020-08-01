#!/bin/bash

set -e

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh