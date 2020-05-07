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

TEMP_PATH="$TMPDIR/debugmypipeline"
mkdir -p $TEMP_PATH

ARTIFACT_PATH="$TEMP_PATH/artifact.zip"
UNZIP_PATH="$TEMP_PATH/unzipped"
CLIENT_PATH="$UNZIP_PATH/dist/debug-my-pipeline--client"

echo "Installing client..."
curl -s https://artifacts.debugmypipeline.com/${PLATFORM_CODE}/artifact.zip -o $ARTIFACT_PATH
unzip -o $ARTIFACT_PATH -d $UNZIP_PATH 1>/dev/null
chmod +x $CLIENT_PATH

DEBUGMYPIPELINE_KEY='__KEY__' $CLIENT_PATH