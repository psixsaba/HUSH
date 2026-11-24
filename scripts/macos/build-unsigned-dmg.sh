#!/bin/sh
# Unsigned disk image: driver, CLI, login agent, and HUSH.app when Tauri has produced it.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "hdiutil is a macOS tool. Run this on a Mac after the driver build." >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
driver="$root/drivers/macos/build/HUSH.driver"
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
app=$(find "$root/target/release/bundle" "$root/apps/desktop/src-tauri/target" -path '*.app' -name 'HUSH.app' -type d 2>/dev/null | head -n 1 || true)
if [ -z "$app" ]; then
    echo "Missing HUSH.app. Build it with: npm run tauri build -- --bundles app" >&2
    exit 1
fi
# ONNX Runtime is loaded from the executable directory.
find "$root/target/release" -name '*onnxruntime*.dylib' -print | while read -r lib; do
    cp -f "$lib" "$app/Contents/MacOS/" || true
done
if command -v codesign >/dev/null 2>&1; then
    if codesign -dv "$app" 2>&1 | grep -q "Authority="; then
        echo "keeping the Developer ID signature on $app"
    else
        codesign --force --deep --sign - "$app" || echo "warning: ad-hoc codesign of HUSH.app failed" >&2
    fi
fi
cp -R "$app" "$stage/HUSH.app"
if [ -f "$root/dist/HUSH_0.3.0_macos.pkg" ]; then
    cp "$root/dist/HUSH_0.3.0_macos.pkg" "$stage/HUSH_0.3.0_macos.pkg"
fi
cp "$root/scripts/macos/uninstall-hush.sh" "$stage/uninstall-hush.sh"
cp "$root/scripts/macos/Uninstall-HUSH.command" "$stage/Uninstall HUSH.command"
chmod 755 "$stage/uninstall-hush.sh" "$stage/Uninstall HUSH.command"
cat > "$stage/Install.txt" <<'EOF'
1. Drag HUSH.app to Applications.
2. Double-click HUSH_0.3.0_macos.pkg. It installs HUSH.driver into
   /Library/Audio/Plug-Ins/HAL and restarts coreaudiod. macOS asks for
   an administrator password. The first launch of HUSH.app can also
   install the driver with the Install driver button.
3. Open HUSH. If macOS says it cannot be opened, right-click HUSH.app,
   choose Open, then Open again. Or, in Terminal:
     xattr -cr /Applications/HUSH.app
   This build is ad-hoc signed. It is not notarized. Notarization needs
   an Apple Developer ID, which this project does not have.
4. Turn HUSH on. In Zoom, Meet, Discord, or Teams select
   HUSH Microphone and HUSH Speaker.
5. Double-click "Uninstall HUSH.command" to remove the driver.

coreaudiod on Apple silicon can still refuse an ad-hoc plugin. If the
devices never appear, System Settings, Privacy & Security may offer an
Allow button for the blocked plugin.
EOF
arch=$(uname -m)
case "$arch" in
    arm64) name="HUSH_0.3.0_aarch64.dmg" ;;
    x86_64) name="HUSH_0.3.0_x86_64.dmg" ;;
    *) name="HUSH_0.3.0_${arch}.dmg" ;;
esac
hdiutil create -volname HUSH -srcfolder "$stage" -ov -format UDZO "$root/dist/$name"
echo "wrote $root/dist/$name"
