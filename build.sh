#!/bin/sh
# Build xsp, install it to ~/.local/bin with its overlay library, and (re)start it as a login agent.
# Run on the Mac.
#
# Signing: CODESIGN_IDENTITY names the identity to sign xsp and its scratchpad wrappers with
# (default "-", ad hoc), optionally found in the keychain CODESIGN_KEYCHAIN, unlocked with
# CODESIGN_KEYCHAIN_PASSWORD. Ad hoc works, but macOS then forgets the Accessibility and App
# Management grants whenever a binary changes; a self-signed Code Signing certificate keeps them,
# because TCC keys on the identifier and the certificate. The daemon re-signs its wrappers itself,
# so the agent passes these variables on. They can live in signing.env next to this script
# (ignored by git), with XSP_ALACRITTY if Alacritty is somewhere unusual.
set -eu
cd "$(dirname "$0")"
[ -f signing.env ] && . ./signing.env
LABEL=xyz.crazyboycjr.xsp
BIN="$HOME/.local/bin/xsp"
IDENTITY="${CODESIGN_IDENTITY:--}"

PATH="$HOME/.cargo/bin:$PATH" cargo build --release
mkdir -p "$HOME/.local/bin"

# The window overlay injected into each scratchpad. Ad-hoc signing is enough: it is loaded through
# DYLD_INSERT_LIBRARIES, which the wrapper binary (no hardened runtime) allows.
clang -dynamiclib -fobjc-arc -O2 -o "$HOME/.local/bin/xsp-overlay.dylib" overlay.c
codesign --force --sign - "$HOME/.local/bin/xsp-overlay.dylib"

cp target/release/xsp "$BIN.new"
if [ -n "${CODESIGN_KEYCHAIN:-}" ]; then
    [ -n "${CODESIGN_KEYCHAIN_PASSWORD:-}" ] && security unlock-keychain -p "$CODESIGN_KEYCHAIN_PASSWORD" "$CODESIGN_KEYCHAIN"
    codesign --force --keychain "$CODESIGN_KEYCHAIN" --sign "$IDENTITY" --identifier "$LABEL" "$BIN.new"
else
    codesign --force --sign "$IDENTITY" --identifier "$LABEL" "$BIN.new"
fi
mv "$BIN.new" "$BIN"
[ "$IDENTITY" = "-" ] && echo "signed ad hoc: macOS will ask for its permissions again after each rebuild" >&2

env_entry() {
    if [ -n "$2" ]; then
        printf '    <key>%s</key><string>%s</string>\n' "$1" "$2"
    fi
}
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
{
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key><array><string>$BIN</string><string>daemon</string></array>
  <key>EnvironmentVariables</key><dict>
EOF
    # XSP_OVERLAY=0 falls back to macOS's own full screen instead of the menu-bar overlay
    env_entry XSP_OVERLAY "${XSP_OVERLAY:-1}"
    env_entry XSP_ALACRITTY "${XSP_ALACRITTY:-}"
    env_entry CODESIGN_IDENTITY "${CODESIGN_IDENTITY:-}"
    env_entry CODESIGN_KEYCHAIN "${CODESIGN_KEYCHAIN:-}"
    env_entry CODESIGN_KEYCHAIN_PASSWORD "${CODESIGN_KEYCHAIN_PASSWORD:-}"
    cat <<EOF
  </dict>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/xsp.log</string>
</dict></plist>
EOF
} > "$PLIST"
chmod 600 "$PLIST"
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
echo "xsp installed at $BIN and started"
