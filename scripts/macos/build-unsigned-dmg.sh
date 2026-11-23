#!/bin/sh
# Unsigned disk image containing the driver bundle, the CLI, and the .app if Tauri produced one.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "hdiutil is a macOS tool. This script is ready to run on a Mac." >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
stage=$(mktemp -d)
mkdir -p "$root/dist"
cp -R "$root/drivers/macos/build/KrispAltMicrophone.driver" "$stage/"
[ -x "$root/target/release/hush" ] && cp "$root/target/release/hush" "$stage/"
app=$(find "$root/apps/desktop/src-tauri/target" -name 'Hush.app' -type d | head -n 1 || true)
if [ -n "$app" ]; then
    cp -R "$app" "$stage/"
fi
cat > "$stage/Install.txt" <<'EOF'
1. Copy KrispAltMicrophone.driver to /Library/Audio/Plug-Ins/HAL/
2. Restart coreaudiod: sudo killall coreaudiod
3. Open Hush.app and press Start.
4. In Zoom, Meet, Discord, or Teams, choose Krisp Alt Microphone and Krisp Alt Speaker.

This image is unsigned. macOS will ask you to approve it.
EOF
hdiutil create -volname Hush -srcfolder "$stage" -ov -format UDZO "$root/dist/Hush-unsigned.dmg"
rm -rf "$stage"
echo "wrote $root/dist/Hush-unsigned.dmg"
