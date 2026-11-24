#!/bin/sh
# Remove the HUSH HAL plugin, the login item, and the CLI copy.
# Run with sudo, or double-click Uninstall HUSH.command on the disk image.
set -eu
if [ "$(id -u)" -ne 0 ]; then
    echo "Re-run with sudo." >&2
    exit 1
fi
rm -rf /Library/Audio/Plug-Ins/HAL/HUSH.driver
rm -f /usr/local/bin/hush
killall coreaudiod 2>/dev/null || true
# Login items live in each user's home. Remove the ones we can see.
for home in /Users/* /var/root; do
    agent="$home/Library/LaunchAgents/com.hush.desktop.plist"
    if [ -f "$agent" ]; then
        uid=$(stat -f %u "$home" 2>/dev/null || true)
        if [ -n "${uid:-}" ]; then
            launchctl bootout "gui/$uid/com.hush.desktop" 2>/dev/null || true
        fi
        rm -f "$agent"
    fi
done
echo "Removed the HUSH driver. /Applications/HUSH.app is left in place; drag it to the Trash to remove the app."
