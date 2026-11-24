#!/bin/sh
# Install a per-user LaunchAgent so HUSH opens at login.
# The app bundle must already be in /Applications/HUSH.app.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "LaunchAgents are a macOS feature. This script is ready to run on a Mac." >&2
    exit 1
fi
if [ ! -d /Applications/HUSH.app ]; then
    echo "Copy HUSH.app to /Applications first." >&2
    exit 1
fi
dest="${HOME}/Library/LaunchAgents"
mkdir -p "$dest"
src=$(CDPATH= cd -- "$(dirname "$0")" && pwd)/com.hush.desktop.plist
cp "$src" "$dest/com.hush.desktop.plist"
launchctl bootout "gui/$(id -u)/com.hush.desktop" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$dest/com.hush.desktop.plist"
echo "HUSH will open at the next login."
