#!/bin/sh
# Build the Krisp Alt Microphone AudioServerPlugIn.
# Apple Silicon and Intel: set HUSH_UNIVERSAL=1 to produce both slices.
set -eu

cd "$(dirname "$0")"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "This driver builds only on macOS."
    exit 1
fi

archs="${CMAKE_OSX_ARCHITECTURES:-}"
if [ "${HUSH_UNIVERSAL:-0}" = "1" ]; then
    archs="arm64;x86_64"
fi

identity="${CODESIGN_ID:--}"
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release \
    -DCODESIGN_ID="$identity" \
    ${archs:+-DCMAKE_OSX_ARCHITECTURES="$archs"}
cmake --build build
echo "built $(pwd)/build/KrispAltMicrophone.driver"
