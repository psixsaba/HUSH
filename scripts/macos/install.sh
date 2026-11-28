#!/bin/sh
# One-step install for macOS.
#   curl -fsSL https://raw.githubusercontent.com/psixsaba/HUSH/main/install.sh | bash
# The copy at the repository root is this same script.
set -eu

repo="${HUSH_REPO:-psixsaba/HUSH}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Looking up the newest HUSH release, including pre-releases."
json=$(curl -fsSL "https://api.github.com/repos/${repo}/releases?per_page=10")
tag=$(printf '%s\n' "$json" | sed -n 's/.*"tag_name": "\([^"]*\)".*/\1/p' | head -n 1)
echo "Installing ${tag:-unknown}"
app_url=$(printf '%s\n' "$json" | sed -n 's/.*"browser_download_url": "\([^"]*HUSH_[^"]*\.app\.tar\.gz\)".*/\1/p' | head -n 1)
driver_url=$(printf '%s\n' "$json" | sed -n 's/.*"browser_download_url": "\([^"]*HUSH\.driver\.tar\.gz\)".*/\1/p' | head -n 1)
if [ -z "$app_url" ] || [ -z "$driver_url" ]; then
    echo "The newest releases do not include HUSH.app.tar.gz and HUSH.driver.tar.gz." >&2
    exit 1
fi
echo "App:    $app_url"
echo "Driver: $driver_url"
curl -fL --retry 3 -o "$tmp/app.tar.gz" "$app_url"
curl -fL --retry 3 -o "$tmp/driver.tar.gz" "$driver_url"
tar -xzf "$tmp/app.tar.gz" -C "$tmp"
tar -xzf "$tmp/driver.tar.gz" -C "$tmp"
if [ ! -d "$tmp/HUSH.app" ] || [ ! -d "$tmp/HUSH.driver" ]; then
    echo "The archives did not contain HUSH.app and HUSH.driver." >&2
    exit 1
fi

echo "Installing HUSH.app and the audio plugin. macOS will ask for an administrator password once."
sudo /bin/sh -c "
    rm -rf /Applications/HUSH.app
    cp -R '$tmp/HUSH.app' /Applications/HUSH.app
    xattr -cr /Applications/HUSH.app
    mkdir -p /Library/Audio/Plug-Ins/HAL
    rm -rf /Library/Audio/Plug-Ins/HAL/HUSH.driver
    cp -R '$tmp/HUSH.driver' /Library/Audio/Plug-Ins/HAL/HUSH.driver
    xattr -cr /Library/Audio/Plug-Ins/HAL/HUSH.driver
    killall coreaudiod || true
"

echo "Waiting for the audio devices."
found=0
for _ in 1 2 3 4 5 6 7 8; do
    if system_profiler SPAudioDataType 2>/dev/null | grep -q "HUSH Microphone"; then
        found=1
        break
    fi
    sleep 1
done
if [ "$found" -eq 1 ]; then
    echo "HUSH Microphone is in the audio device list."
    echo "Open HUSH, turn it on, and in Zoom, Meet, Discord, or Teams select HUSH Microphone and HUSH Speaker."
    exit 0
fi
cat <<'EOF' >&2
HUSH is installed, but coreaudiod has not listed HUSH Microphone yet.

On macOS 15 Sequoia, try to open HUSH once. Then go to
System Settings > Privacy & Security > Open Anyway.
If the audio plugin is the piece that was blocked, use that same
Open Anyway button for it, then run this installer again or:
    sudo killall coreaudiod
EOF
exit 1
