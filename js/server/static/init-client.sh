#!/bin/sh
## === DEBUGMYPIPELINE SHELL SCRIPT ===

case "$(uname -s):$(uname -m)" in
Linux:x86_64*)     
    TARGET="x86_64-unknown-linux-gnu"
    ;;
Linux:arm*)     
    TARGET="armv7-unknown-linux-gnueabihf"
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
    TMPDIR="/tmp/"
fi

TEMP_PATH="$TMPDIR/debugmypipeline"
CLIENT_PATH="$TEMP_PATH/client"

mkdir -p $TEMP_PATH

echo "Installing client..."
if [ ! -z "$(which curl)" ]
then
    curl -s https://artifacts.debugmypipeline.com/${TARGET}-client -o $CLIENT_PATH 
else
    wget https://artifacts.debugmypipeline.com/${TARGET}-client -O $CLIENT_PATH 2> /dev/null
fi
chmod +x $CLIENT_PATH

DEBUGMYPIPELINE_KEY='__KEY__' $CLIENT_PATH