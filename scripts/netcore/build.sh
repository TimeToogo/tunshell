#!/bin/bash

set -e

DIR=$(dirname $0)

dotnet build --configuration Release
cp $DIR/bin/Release/netcoreapp3.1/init.dll $DIR/../init.dotnet.dll