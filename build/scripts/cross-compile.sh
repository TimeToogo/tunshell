#!/bin/bash

set -e

echo "Parsing targets..."
TARGETS=$(cat targets.json | jq -r '.[] | [.machine, .arch, .cc, .cc_toolchain, .ldflags, .cflags, .rust_target] | @tsv')
TARGETS=${TARGETS//$'\t'/,}


echo "$TARGETS" | while IFS=',' read -r MACHINE ARCH CC CC_TOOLCHAIN LDFLAGS CFLAGS RUST_TARGET
do
   echo "Building $RUST_TARGET..."

   export MACHINE
   export ARCH
   export CC
   export LD=$CC
   export LDFLAGS
   export CFLAGS

   echo "Installing rust target..."
   rustup target add $RUST_TARGET

   cat > /workspace/dmp-client/.cargo/config <<< EOF
[target.$RUST_TARGET]
linker = "$CC"
EOF

   echo "Cross-compiling OpenSSL..."
   OPENSSL_BUILD_DIR=/tmp/openssl-build-$CC_TOOLCHAIN/
   mkdir -p $OPENSSL_BUILD_DIR
   cd /tmp/openssl/
   ./config shared --openssldir=$OPENSSL_BUILD_DIR --prefix=$OPENSSL_BUILD_DIR
   make install

   echo "Cross-compiling Libsodium..."
   LIBSODIUM_BUILD_DIR=/tmp/libsodium-build-$CC_TOOLCHAIN/
   mkdir -p $LIBSODIUM_BUILD_DIR
   cd /tmp/libsodium/
   ./configure --host=$CC_TOOLCHAIN --prefix=$LIBSODIUM_BUILD_DIR
   make install

   export OPENSSL_LIB_DIR="$OPENSSL_BUILD_DIR"
   export OPENSSL_INCLUDE_DIR="/tmp/openssl/include"
   export SODIUM_LIB_DIR="$LIBSODIUM_BUILD_DIR/lib"
   export OPENSSL_STATIC=1
   export SODIUM_STATIC=1

   echo "Compiling dmp-client for $RUST_TARGET..."
   cd /workspace/dmp-client
   cargo build --target $RUST_TARGET
done