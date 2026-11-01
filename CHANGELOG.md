# Changelog

## 1.0.0 (2026-10-31)

The first public release: xmonad's exclusive scratchpads for macOS, three
Alacritty terminals on F1, F3 and F4. Install it on macOS 27, with Alacritty,
Rust and the Xcode Command Line Tools:

```sh
git clone https://github.com/crazyboycjr/xsp
cd xsp
sh build.sh
```

Allow Accessibility when macOS asks. To keep the permissions across builds, sign
with a certificate of your own through `CODESIGN_IDENTITY`; the README's
[Signing](README.md#signing) section shows how. With
[ZeroSpace](https://github.com/crazyboycjr/zerospace) running, pads come to the
desktop you are on.

**The keys.** F1, F3 and F4 toggle scratch-0, scratch-1 and scratch-2. A key
starts its pad the first time, puts it away when it is out on this desktop, and
otherwise brings it to this desktop with the keyboard. Bringing up one pad puts
away any other out on the same desktop.

**Getting out of the way.** A pad that is out hides when you switch to another
app on its desktop, by cmd+tab, a click or a switcher such as AltTab, so the app
you switched to comes forward. Leaving the desktop keeps the pad there.

**Out of the Dock.** Each pad is Alacritty under its own bundle id,
`xyz.crazyboycjr.xsp.scratch-N`, in an accessory app xsp keeps in
`~/Library/Application Support/xsp/`, hidden through Accessibility and shown
with a LaunchServices reopen. Until Accessibility is granted, the pads are
ordinary Dock apps.

**On this desktop.** Before showing a pad that is on another desktop, xsp asks
ZeroSpace to move its window here. Without ZeroSpace, showing it switches to its
desktop.

**Over the menu bar.** By default a pad covers the whole screen, above the menu
bar, through `xsp-overlay.dylib`, loaded into the pads alone; `XSP_OVERLAY=0`
leaves the pads to Alacritty's configuration.

**Installing.** `build.sh` installs `~/.local/bin/xsp` and the login agent
`xyz.crazyboycjr.xsp`. It and the agent sign ad hoc by default, or with the
identity in `CODESIGN_IDENTITY`, optionally from the keychain
`CODESIGN_KEYCHAIN` unlocked with `CODESIGN_KEYCHAIN_PASSWORD`. Alacritty comes
from `XSP_ALACRITTY`, or the first of `/Applications`, `~/Applications` and
`/Applications/Nix Apps` that has it. Any of these can live in `signing.env`
beside `build.sh`.

**Command line.** `xsp toggle N` does what a key does, and `xsp status` shows
each pad.

Tested on macOS 27.0.1 on Apple Silicon.
