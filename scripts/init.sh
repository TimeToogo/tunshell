#!/bin/sh
## === TUNSHELL SHELL SCRIPT ===

set -e

case "$(uname -s):$(uname -m)" in
Linux:x86_64*)     
    TARGET="x86_64-unknown-linux-musl"
    ;;
Linux:arm64*)     
    TARGET="aarch64-unknown-linux-musl"
    ;;
Linux:arm*)     
    TARGET="armv7-unknown-linux-musleabihf"
    ;;
Linux:i686*)     
    TARGET="i686-unknown-linux-musl"
    ;;
Linux:i586*)     
    TARGET="i586-unknown-linux-musl"
    ;;
Darwin:x86_64*)    
    TARGET="x86_64-apple-darwin"
    ;;
WindowsNT:x86_64*)    
    TARGET="x86_64-pc-windows-msvc.exe"
    ;;
WindowsNT:i686*)    
    TARGET="i686-pc-windows-msvc.exe"
    ;;
*)          
    echo "Unsupported system ($(uname))"
    exit 1
    ;;
esac

if [ -z "$TMPDIR" ]
then
    TMPDIR="/tmp"
fi

TEMP_PATH="$TMPDIR/tunshell"
CLIENT_PATH="$TEMP_PATH/client"
HEADERS_PATH="$TEMP_PATH/headers"

mkdir -p $TEMP_PATH


if [ -x "$(command -v curl)" ]
then
    INSTALL_CLIENT=true

    # Check if client is already downloaded and up-to-date
    if [ -x "$(command -v grep)" ] && [ -x "$(command -v cut)" ] && [ -x "$(command -v sed)" ] && [ -f "$HEADERS_PATH" ]
    then
        CURRENT_ETAG=$(cat $HEADERS_PATH | grep -i etag || true)
        LATEST_ETAG=$(curl -XHEAD -sSfI https://artifacts.tunshell.com/client-${TARGET} | grep -i etag || true)

        if [ "$CURRENT_ETAG" = "$LATEST_ETAG" ]
        then
            echo "Client already installed..."
            INSTALL_CLIENT=false
        fi
    fi

    if [ "$INSTALL_CLIENT" = true ]
    then
        echo "Installing client..."
        curl -sSf https://artifacts.tunshell.com/client-${TARGET} -o $CLIENT_PATH -D $HEADERS_PATH
    fi
else
    wget https://artifacts.tunshell.com/client-${TARGET} -O $CLIENT_PATH 2> /dev/null
fi
chmod +x $CLIENT_PATH

$CLIENT_PATH $1 $2 $3 $4 $5 $6 $7 $8 $9 
