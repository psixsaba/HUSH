#!/bin/sh
set -eu
if [ "$(id -u)" -ne 0 ]; then
    echo "Re-run with sudo." >&2
    exit 1
fi
rm -rf /Library/Audio/Plug-Ins/HAL/KrispAltMicrophone.driver
launchctl kickstart -k system/com.apple.audio.coreaudiod || killall -9 coreaudiod || true
echo "Removed Krisp Alt Microphone"
