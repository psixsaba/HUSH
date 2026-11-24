#!/bin/sh
# Install HUSH Microphone and HUSH Speaker. Requires a built driver bundle.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
bundle="$root/drivers/macos/build/HUSH.driver"
dest="/Library/Audio/Plug-Ins/HAL/HUSH.driver"
if [ ! -d "$bundle" ]; then
    echo "Build the driver first: drivers/macos/build.sh" >&2
    exit 1
fi
if [ "$(id -u)" -ne 0 ]; then
    echo "Re-run with sudo." >&2
    exit 1
fi
rm -rf "$dest"
cp -R "$bundle" "$dest"
launchctl kickstart -k system/com.apple.audio.coreaudiod || killall -9 coreaudiod || true
echo "Installed $dest"
echo "In a call app, select HUSH Microphone and HUSH Speaker."
