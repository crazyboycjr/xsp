# xsp

xmonad's exclusive scratchpads for Alacritty on macOS, on the same keys: F1, F3 and F4 toggle scratchpad 0, 1 and 2 (`alacritty --title scratch-N`, with the usual `~/.alacritty.toml`).

A key spawns its scratchpad when it has no window yet. When the scratchpad is shown on the current Space, the key hides it, and the focus goes back to the app that had it before. Otherwise (hidden, or shown on another Space) the key brings it to the current Space and focuses it. Showing one scratchpad hides the others shown on the same Space, and a scratchpad stays on the Space it was shown on when you switch away.

## How

Each scratchpad is Alacritty under its own bundle id (`local.xsp.scratch-N`), in a wrapper bundle the daemon keeps in `~/Library/Application Support/xsp/`, so LaunchServices can hide, unhide and activate exactly that one. The wrappers hold a copy of the nix Alacritty binary, refreshed whenever nix replaces it. The keys are Carbon hot keys, so the focused app never sees them and xsp needs no event tap. Windows move between Spaces through ZeroSpace's socket (`move WID N`). xsp needs no permission of its own. Without ZeroSpace, a scratchpad on another Space stays there, and showing it switches to that Space.

## Install (on the Mac)

```sh
cargo build --release
mkdir -p ~/.local/bin
cp target/release/xsp ~/.local/bin/xsp.new && mv ~/.local/bin/xsp.new ~/.local/bin/xsp
cp local.xsp.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/local.xsp.plist
```

After a rebuild, copy the binary the same way and restart the agent with `launchctl kickstart -k gui/$(id -u)/local.xsp`. Stop it with `launchctl bootout gui/$(id -u)/local.xsp`.

## Use

```sh
xsp toggle 0   # what F1 does; 1 is F3, 2 is F4
xsp status     # each scratchpad's pid, hidden state, window, Space
```

The log is `~/Library/Logs/xsp.log`.
