#!/bin/sh
# Compile Scala 3 sources with built-in SemanticDB: scala-semanticdb CLASSES_DIR SEMANTICDB_DIR SOURCE...
# Run from the source root; SOURCE paths are relative to it. The compiler and library jars are the image's.
set -eu
classes=$1
semanticdb=$2
shift 2
mkdir -p "$classes" "$semanticdb"
exec java -cp "$(cat /opt/scala-compiler-classpath.txt)" dotty.tools.dotc.Main \
  -classpath "$(cat /opt/scala-library-classpath.txt)" -d "$classes" \
  -sourceroot "$PWD" -Xsemanticdb -semanticdb-target "$semanticdb" "$@"
