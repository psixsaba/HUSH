#!/bin/sh
# Remove HUSH.app and the HAL plugin. One administrator prompt.
set -eu
echo "Removing HUSH. macOS will ask for an administrator password."
sudo /bin/sh -c '
    rm -rf /Applications/HUSH.app
    rm -rf /Library/Audio/Plug-Ins/HAL/HUSH.driver
    killall coreaudiod || true
'
for home in /Users/* "$HOME"; do
    agent="$home/Library/LaunchAgents/com.hush.desktop.plist"
    if [ -f "$agent" ]; then
        uid=$(stat -f %u "$home" 2>/dev/null || true)
        if [ -n "${uid:-}" ]; then
            launchctl bootout "gui/$uid/com.hush.desktop" 2>/dev/null || true
        fi
        rm -f "$agent"
    fi
done
echo "HUSH is removed."
