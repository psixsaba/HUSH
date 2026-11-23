#!/bin/sh
# Unsigned disk image: driver, CLI, login agent, and Hush.app when Tauri has produced it.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "hdiutil is a macOS tool. Run this on a Mac after the driver build." >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
driver="$root/drivers/macos/build/KrispAltMicrophone.driver"
if [ ! -d "$driver" ]; then
    echo "Missing $driver. Build it with drivers/macos/build.sh." >&2
    exit 1
fi
mkdir -p "$root/dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -R "$driver" "$stage/"
cp "$root/scripts/macos/com.hush.desktop.plist" "$stage/"
cp "$root/scripts/macos/install-login-item.sh" "$stage/"
chmod 755 "$stage/install-login-item.sh"
if [ -x "$root/target/release/hush" ]; then
    cp "$root/target/release/hush" "$stage/hush"
fi
app=$(find "$root/target" "$root/apps/desktop/src-tauri/target" -name 'Hush.app' -type d 2>/dev/null | head -n 1 || true)
if [ -n "$app" ]; then
    cp -R "$app" "$stage/Hush.app"
fi
cat > "$stage/Install.txt" <<'EOF'
1. Copy KrispAltMicrophone.driver to /Library/Audio/Plug-Ins/HAL/
   or install Hush-unsigned.pkg if you built it.
2. Restart coreaudiod: sudo killall coreaudiod
3. Copy Hush.app to /Applications if it is in this image.
4. ./install-login-item.sh opens Hush at login.
5. In Zoom, Meet, Discord, or Teams choose Krisp Alt Microphone and Krisp Alt Speaker.

This image is unsigned. macOS will ask you to approve it. A plugin that
coreaudiod loads on Apple Silicon usually needs a Developer ID signature.
EOF
hdiutil create -volname Hush -srcfolder "$stage" -ov -format UDZO "$root/dist/Hush-unsigned.dmg"
echo "wrote $root/dist/Hush-unsigned.dmg"
