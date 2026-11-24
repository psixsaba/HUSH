#!/bin/sh
# Double-click uninstaller. macOS asks for an administrator password.
set -eu
cd "$(dirname "$0")"
script="$(pwd)/uninstall-hush.sh"
if [ ! -f "$script" ]; then
    script=/usr/local/share/hush/uninstall-hush.sh
fi
osascript -e "do shell script \"/bin/sh '$script'\" with administrator privileges"
echo "HUSH driver removed."
