#!/bin/sh
# Developer ID signing and notarization stay off unless the secrets exist.
# The ad-hoc signature from the package scripts remains in that case.
set -eu
if [ -z "${APPLE_CERTIFICATE_P12:-}" ] || [ -z "${APPLE_CERTIFICATE_PASSWORD:-}" ] || [ -z "${APPLE_SIGNING_IDENTITY:-}" ] || [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ] || [ -z "${APPLE_APP_PASSWORD:-}" ]; then
    echo "Developer ID signing is off. APPLE_CERTIFICATE_P12, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_TEAM_ID, and APPLE_APP_PASSWORD are not all set."
    exit 0
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
app=$(find "$root/target/release/bundle" -name 'HUSH.app' -type d | head -n 1)
driver="$root/drivers/macos/build/HUSH.driver"
keychain="$RUNNER_TEMP/hush-signing.keychain"
printf '%s' "$APPLE_CERTIFICATE_P12" | base64 --decode > "$RUNNER_TEMP/certificate.p12"
security create-keychain -p hush-sign "$keychain"
security default-keychain -s "$keychain"
security unlock-keychain -p hush-sign "$keychain"
security import "$RUNNER_TEMP/certificate.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k hush-sign "$keychain"
codesign --force --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$driver"
codesign --force --deep --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$app"
ditto -c -k --keepParent "$app" "$RUNNER_TEMP/HUSH.zip"
xcrun notarytool submit "$RUNNER_TEMP/HUSH.zip" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
xcrun stapler staple "$app"
echo "Developer ID signature and notarization finished."
