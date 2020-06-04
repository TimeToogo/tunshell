#!/bin/sh
## === DEBUGMYPIPELINE SHELL SCRIPT ===

set -e

case "$(uname -s):$(uname -m)" in
Linux:x86_64*)     
    TARGET="x86_64-unknown-linux-musl"
    ;;
Linux:arm*)     
    TARGET="armv7-unknown-linux-musleabihf"
    ;;
Darwin:x86_64*)    
    TARGET="x86_64-apple-darwin"
    ;;
WindowsNT:x86_64*)    
    TARGET="x86_64-pc-windows-gnu"
    ;;
WindowsNT:i686*)    
    TARGET="i686-pc-windows-gnu"
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

TEMP_PATH="$TMPDIR/debugmypipeline"
CLIENT_PATH="$TEMP_PATH/client"

mkdir -p $TEMP_PATH


echo "Installing client..."
if [ -x "$(command -v curl)" ]
then
    curl -sSf https://artifacts.debugmypipeline.com/client-${TARGET} -o $CLIENT_PATH 
else
    wget https://artifacts.debugmypipeline.com/client-${TARGET} -O $CLIENT_PATH 2> /dev/null
fi
chmod +x $CLIENT_PATH

DMP_KEY='__KEY__' $CLIENT_PATH