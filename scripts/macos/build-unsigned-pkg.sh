#!/bin/sh
# Unsigned installer for the HAL plugin and the hush CLI.
# pkgbuild exists only on macOS. The package is not notarized.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "pkgbuild is a macOS tool. Run this on a Mac after drivers/macos/build.sh." >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
driver="$root/drivers/macos/build/HUSH.driver"
if [ ! -d "$driver/Contents/MacOS" ]; then
    echo "Missing $driver. Build it with drivers/macos/build.sh." >&2
    exit 1
fi
mkdir -p "$root/dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
payload="$stage/root"
mkdir -p "$payload/Library/Audio/Plug-Ins/HAL" "$payload/usr/local/bin" "$payload/usr/local/share/hush"
cp -R "$driver" "$payload/Library/Audio/Plug-Ins/HAL/"
cp "$root/scripts/macos/com.hush.desktop.plist" "$payload/usr/local/share/hush/"
if [ -x "$root/target/release/hush" ]; then
    cp "$root/target/release/hush" "$payload/usr/local/bin/hush"
else
    echo "warning: target/release/hush is missing; the package will only install the driver" >&2
fi
mkdir -p "$stage/scripts"
cat > "$stage/scripts/postinstall" <<'EOF'
#!/bin/sh
launchctl kickstart -k system/com.apple.audio.coreaudiod || killall -9 coreaudiod || true
exit 0
EOF
chmod 755 "$stage/scripts/postinstall"
# Ad-hoc sign the bundle inside the payload so a local install has a signature.
# coreaudiod on Apple Silicon may still refuse it without a Developer ID.
if command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - "$payload/Library/Audio/Plug-Ins/HAL/HUSH.driver" || \
        echo "warning: ad-hoc codesign failed; the plugin may not load" >&2
fi
pkgbuild \
    --root "$payload" \
    --scripts "$stage/scripts" \
    --identifier com.hush.audio \
    --version 0.1.0 \
    --install-location / \
    "$root/dist/HUSH-unsigned.pkg"
echo "wrote $root/dist/HUSH-unsigned.pkg"
