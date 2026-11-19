# xsp and its overlay library as build.sh installs them, built by nix and signed ad hoc:
# bin/xsp, and lib/xsp-overlay.dylib, where xsp looks for it beside its own package.
{ lib, rustPlatform, apple-sdk_26, rcodesign }:

rustPlatform.buildRustPackage {
  pname = "xsp";
  inherit ((lib.importTOML ./Cargo.toml).package) version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./build.rs
      ./src
      ./overlay.c
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  # rcodesign signs ad hoc without Apple's codesign, which the build cannot reach
  nativeBuildInputs = [ rcodesign ];
  # The SDK build.sh gets from the Command Line Tools
  buildInputs = [ apple-sdk_26 ];

  doCheck = false; # no tests

  # The window overlay injected into each scratchpad, built as build.sh builds it
  postBuild = ''
    $CC -dynamiclib -fobjc-arc -O2 -o xsp-overlay.dylib overlay.c
  '';

  postInstall = ''
    install -Dm755 xsp-overlay.dylib $out/lib/xsp-overlay.dylib
  '';

  # After stripping, which rewrites the binaries. The identifier is the label, as build.sh signs
  # it, and neither gets the hardened runtime.
  postFixup = ''
    rcodesign sign --binary-identifier xyz.crazyboycjr.xsp $out/bin/xsp
    rcodesign sign $out/lib/xsp-overlay.dylib
  '';

  meta = {
    description = "xmonad's exclusive scratchpads for Alacritty on macOS";
    homepage = "https://github.com/crazyboycjr/xsp";
    license = lib.licenses.mit;
    mainProgram = "xsp";
    platforms = lib.platforms.darwin;
  };
}
