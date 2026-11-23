#!/bin/sh
# Assemble an unsigned .pkg. pkgbuild exists only on macOS.
# The package installs the HAL driver. The app is copied beside it for a drag-install later.
set -eu
if [ "$(uname -s)" != "Darwin" ]; then
    echo "pkgbuild is a macOS tool. This script is ready to run on a Mac." >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
stage=$(mktemp -d)
payload="$stage/root"
mkdir -p "$payload/Library/Audio/Plug-Ins/HAL" "$payload/usr/local/bin"
cp -R "$root/drivers/macos/build/KrispAltMicrophone.driver" "$payload/Library/Audio/Plug-Ins/HAL/"
if [ -x "$root/target/release/hush" ]; then
    cp "$root/target/release/hush" "$payload/usr/local/bin/hush"
fi
mkdir -p "$stage/scripts"
cat > "$stage/scripts/postinstall" <<'EOF'
#!/bin/sh
launchctl kickstart -k system/com.apple.audio.coreaudiod || killall -9 coreaudiod || true
exit 0
EOF
chmod 755 "$stage/scripts/postinstall"
pkgbuild \
    --root "$payload" \
    --scripts "$stage/scripts" \
    --identifier com.hush.audio \
    --version 0.1.0 \
    --install-location / \
    "$root/dist/Hush-unsigned.pkg"
echo "wrote $root/dist/Hush-unsigned.pkg"
rm -rf "$stage"
