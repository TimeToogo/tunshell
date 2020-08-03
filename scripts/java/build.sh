#!/bin/bash

set -e

DIR=$(dirname $0)

javac -d $DIR/bin init.java
cd $DIR/bin
jar cvfm $DIR/../../init.jar ../Manifest.txt .
cd $DIR