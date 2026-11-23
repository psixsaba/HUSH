#!/bin/sh
# Install Krisp Alt Microphone and Krisp Alt Speaker. Requires a built driver bundle.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
bundle="$root/drivers/macos/build/KrispAltMicrophone.driver"
dest="/Library/Audio/Plug-Ins/HAL/KrispAltMicrophone.driver"
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
echo "In a call app, select Krisp Alt Microphone and Krisp Alt Speaker."
