#!/bin/sh
# Build xsp, sign it with the shared self-signed identity so its Accessibility grant survives
# rebuilds (TCC keys on the identifier plus the leaf certificate, not the binary's hash), install
# it to ~/.local/bin, and (re)start it as the login agent local.xsp. Run on the Mac.
set -eu
cd "$(dirname "$0")"
BIN="$HOME/.local/bin/xsp"
KEYCHAIN="$HOME/Library/Keychains/signing.keychain-db"
IDENTITY="Local Code Signing"

PATH="$HOME/.cargo/bin:$PATH" cargo build --release
mkdir -p "$HOME/.local/bin"
cp target/release/xsp "$BIN.new"
if [ -f "$KEYCHAIN" ]; then
    security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
    codesign --force --keychain "$KEYCHAIN" --sign "$IDENTITY" --identifier local.xsp "$BIN.new"
else
    echo "no $KEYCHAIN, signing ad hoc (the Accessibility grant will not survive a rebuild)" >&2
    codesign --force --sign - --identifier local.xsp "$BIN.new"
fi
mv "$BIN.new" "$BIN"

PLIST="$HOME/Library/LaunchAgents/local.xsp.plist"
cp local.xsp.plist "$PLIST"
launchctl bootout "gui/$(id -u)/local.xsp" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
echo "xsp installed at $BIN and started"
