#!/bin/sh
# Fail if THIRD_PARTY_NOTICES drops a shipped dependency or names a GPL license as shipped.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
notices="$root/THIRD_PARTY_NOTICES"
license="$root/LICENSE"
vad_license="$root/drivers/windows/Virtual-Audio-Driver/LICENSE"
vad_mspl="$root/drivers/windows/Virtual-Audio-Driver/THIRD_PARTY_NOTICES.md"

for path in "$notices" "$license" "$vad_license" "$vad_mspl"; do
    if [ ! -s "$path" ]; then
        echo "missing $path" >&2
        exit 1
    fi
done

require() {
    if ! grep -q "$1" "$notices"; then
        echo "THIRD_PARTY_NOTICES does not mention: $1" >&2
        exit 1
    fi
}

require "Virtual-Audio-Driver"
require "MIT"
require "MS-PL"
require "libASPL"
require "DPDFNet"
require "Apache-2.0"
require "ONNX Runtime"

if grep -E -q 'GPL-2|GPL-3|GNU General Public' "$notices"; then
    echo "THIRD_PARTY_NOTICES must not list a GPL component as shipped" >&2
    exit 1
fi

if ! grep -q "MIT License" "$vad_license"; then
    echo "Virtual-Audio-Driver LICENSE is not the MIT text" >&2
    exit 1
fi
if ! grep -q "Microsoft Public License" "$vad_mspl"; then
    echo "Virtual-Audio-Driver third-party file is missing the MS-PL text" >&2
    exit 1
fi

echo "third-party notices ok"
