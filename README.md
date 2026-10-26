# xsp

xmonad's exclusive scratchpads for Alacritty on macOS, on the same keys: F1, F3 and F4 toggle scratchpad 0, 1 and 2 (`alacritty --title scratch-N`, with the usual `~/.alacritty.toml`).

A key spawns its scratchpad when it has no window yet. When the scratchpad is shown on the current Space, the key hides it, and the focus goes back to the app that had it before. Otherwise (hidden, or shown on another Space) the key brings it to the current Space and focuses it. Showing one scratchpad hides the others shown on the same Space, and a scratchpad stays on the Space it was shown on when you switch away.

## How

Each scratchpad is Alacritty under its own bundle id (`local.xsp.scratch-N`), in a wrapper bundle the daemon keeps in `~/Library/Application Support/xsp/`, so LaunchServices and Accessibility can hide, show and activate exactly that one. The wrappers hold a copy of the nix Alacritty binary, refreshed whenever nix replaces it. The keys are Carbon hot keys, so the focused app never sees them and xsp installs no event tap. Windows move between Spaces through ZeroSpace's socket (`move WID N`). Without ZeroSpace, a scratchpad on another Space stays there, and showing it switches to that Space.

The scratchpads stay out of the Dock and Cmd+Tab (they are `LSUIElement` accessory apps). macOS will not let one process hide another's accessory app through the ordinary call, so xsp hides them with Accessibility (`kAXHidden`, the way Cmd+H does) and shows them with a LaunchServices reopen, which needs no permission. So xsp asks for Accessibility once, on first run. Until the grant lands, xsp keeps working with the scratchpads as ordinary Dock apps and the plain hide call, and it switches to accessory apps and `kAXHidden` the moment the grant is there, so nothing breaks in between or if the grant is ever revoked.

The grant survives rebuilds because the binary is signed with a fixed self-signed identity (`Local Code Signing`), so TCC keeps the grant across rebuilds. `build.sh` signs it; without the keychain it falls back to an ad-hoc signature and the grant is lost on each rebuild.

## Install (on the Mac)

```sh
sh build.sh
```

This builds, signs, installs `~/.local/bin/xsp`, writes the LaunchAgent `local.xsp`, and starts it. The first start shows one Accessibility prompt; allow it. Stop it with `launchctl bootout gui/$(id -u)/local.xsp`.

## Use

```sh
xsp toggle 0   # what F1 does; 1 is F3, 2 is F4
xsp status     # each scratchpad's pid, hidden state, window, Space
```

The log is `~/Library/Logs/xsp.log`.
