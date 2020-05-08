## === DEBUGMYPIPELINE SHELL SCRIPT ===

case "$(uname -s)" in
    Linux*)     
    PLATFORM_CODE="ubuntu-latest"
    ;;
    Darwin*)    
    PLATFORM_CODE="macos-latest"
    ;;
    CYGWIN*|MINGW32*|MSYS*|MINGW*)
    PLATFORM_CODE="windows-latest"
    ;;
    *)          
    echo "Unknown operating system, please run on Linux or MacOs..."
    exit 1
    ;;
esac

if [ -z "$TMPDIR" ]
then
    TMPDIR="/tmp/"
fi

TEMP_PATH="$TMPDIR/debugmypipeline"

ARTIFACT_PATH="$TEMP_PATH/artifact.tar.gz"
EXTRACT_PATH="$TEMP_PATH/extracted"
NODE_PATH="$EXTRACT_PATH/node"
BUNDLE_PATH="$EXTRACT_PATH/bundle.js"

mkdir -p $TEMP_PATH
mkdir -p $EXTRACT_PATH

echo "Installing client..."
curl -s https://artifacts.debugmypipeline.com/${PLATFORM_CODE}/artifact.tar.gz -o $ARTIFACT_PATH
tar xzf $ARTIFACT_PATH -C $EXTRACT_PATH 1>/dev/null
chmod +x $NODE_PATH

DEBUGMYPIPELINE_KEY='__KEY__' $NODE_PATH $BUNDLE_PATH