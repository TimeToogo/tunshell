#!/bin/sh
## === TUNSHELL SHELL SCRIPT ===

set -e

case "$(uname -s):$(uname -m)" in
Linux:x86_64*)     
    TARGET="x86_64-unknown-linux-musl"
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
    TARGET="x86_64-pc-windows-msvc"
    ;;
WindowsNT:i686*)    
    TARGET="i686-pc-windows-msvc"
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

mkdir -p $TEMP_PATH


echo "Installing client..."
if [ -x "$(command -v curl)" ]
then
    curl -sSf https://artifacts.tunshell.com/client-${TARGET} -o $CLIENT_PATH 
else
    wget https://artifacts.tunshell.com/client-${TARGET} -O $CLIENT_PATH 2> /dev/null
fi
chmod +x $CLIENT_PATH

TUNSHELL_KEY='__KEY__' $CLIENT_PATH

