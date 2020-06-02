#!/bin/bash

set -e

TARGETS=$1

if [[ ! -f "$TARGETS" ]]; then
   echo "usage: ./cross-compile.sh targets.[host].json"
   exit 1
fi

TEMPDIR=${TEMPDIR:="$(dirname $0)/tmp"}
TEMPDIR=`cd $TEMPDIR;pwd`
echo $TEMPDIR;
source $HOME/.cargo/env

echo "Parsing targets..."
SCRIPT_DIR=$(dirname "$0")
SCRIPT_DIR=`cd $SCRIPT_DIR;pwd`
TARGETS=$(cat $TARGETS | jq -r '.[] | [.openssl_target, .libsodium_target, .cc, .ldflags, .cflags, .rust_target] | @tsv')
TARGETS=${TARGETS//$'\t'/,}

echo "$TARGETS" | while IFS=',' read -r OPENSSL_TARGET LIBSODIUM_TARGET CC LDFLAGS CFLAGS RUST_TARGET
do
   echo "Building $RUST_TARGET..."

   export CC=$"$CC"
   export LD="$CC"
   export LDFLAGS
   export CFLAGS

   echo "Installing rust target..."
   rustup target add $RUST_TARGET

   cat > $SCRIPT_DIR/../dmp-client/.cargo/config << EOF
[target.$RUST_TARGET]
linker = "$CC"
EOF

   OPENSSL_BUILD_DIR=$TEMPDIR/build/openssl-$RUST_TARGET
   if [[ ! -d "$OPENSSL_BUILD_DIR/lib" ]]; then
      echo "Cross-compiling OpenSSL..."
      mkdir -p $OPENSSL_BUILD_DIR
      cd $TEMPDIR/openssl/
      ./Configure shared $OPENSSL_TARGET --openssldir=$OPENSSL_BUILD_DIR --prefix=$OPENSSL_BUILD_DIR 
      make clean install_sw
   fi

   LIBSODIUM_BUILD_DIR=$TEMPDIR/build/libsodium-$RUST_TARGET
   if [[ ! -d "$LIBSODIUM_BUILD_DIR/lib" ]]; then
      echo "Cross-compiling Libsodium..."
      mkdir -p $LIBSODIUM_BUILD_DIR
      cd $TEMPDIR/libsodium/
      unset LD
      ./configure --host=$LIBSODIUM_TARGET --prefix=$LIBSODIUM_BUILD_DIR
      make clean install
      export LD="$CC"
   fi

   export OPENSSL_LIB_DIR="$OPENSSL_BUILD_DIR/lib"
   export OPENSSL_INCLUDE_DIR="$OPENSSL_BUILD_DIR/include"
   export SODIUM_LIB_DIR="$LIBSODIUM_BUILD_DIR/lib"
   export OPENSSL_STATIC=1
   export SODIUM_STATIC=1

   echo "Compiling dmp-client for $RUST_TARGET..."
   cd $SCRIPT_DIR/../dmp-client
   cargo build --target $RUST_TARGET
done