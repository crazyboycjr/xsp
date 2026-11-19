{
  description = "xsp: xmonad's exclusive scratchpads for Alacritty on macOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    {
      # Apple Silicon only: nixpkgs dropped x86_64-darwin in 26.11
      packages.aarch64-darwin.default = nixpkgs.legacyPackages.aarch64-darwin.callPackage ./package.nix { };

      # The login agent build.sh writes, for nix-darwin: same label and environment, started at
      # login and kept running, logging to ~/Library/Logs/xsp.log of the system's primary user.
      darwinModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.services.xsp;
          user = config.system.primaryUser;
          home = if user == null then "" else config.users.users.${user}.home or null;
          optionalEnv = name: value: lib.optionalAttrs (value != null) { ${name} = value; };
        in
        {
          options.services.xsp = {
            enable = lib.mkEnableOption "xsp, scratchpads for Alacritty, as a login agent";

            package = lib.mkOption {
              type = lib.types.package;
              default = pkgs.callPackage ./package.nix { };
              defaultText = lib.literalMD "xsp built with this system's nixpkgs";
              description = "The xsp package to run.";
            };

            alacritty = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "/Applications/Nix Apps/Alacritty.app";
              description = ''
                Alacritty.app's path (`XSP_ALACRITTY`). By default xsp looks in /Applications,
                ~/Applications and /Applications/Nix Apps, where nix-darwin puts it.
              '';
            };

            overlay = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = ''
                Raise each pad over the menu bar with the overlay library (`XSP_OVERLAY`). When
                false, pads open as your Alacritty configuration says.
              '';
            };

            codesignIdentity = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "Local Code Signing";
              description = ''
                The identity xsp signs the pads' apps with (`CODESIGN_IDENTITY`); ad hoc when null.
                A self-signed certificate keeps each pad's permission grants across Alacritty updates.
              '';
            };

            codesignKeychain = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = ''
                The keychain that holds the identity (`CODESIGN_KEYCHAIN`). There is no option for
                its password: it would land in the world-readable nix store.
              '';
            };

            logFile = lib.mkOption {
              type = lib.types.str;
              default = "${if home == null then "/Users/${user}" else home}/Library/Logs/xsp.log";
              defaultText = lib.literalExpression ''"<home of system.primaryUser>/Library/Logs/xsp.log"'';
              description = "Where the agent's standard error goes. launchd does not expand `~`.";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];

            launchd.user.agents.xsp.serviceConfig = {
              Label = "xyz.crazyboycjr.xsp";
              ProgramArguments = [ "${cfg.package}/bin/xsp" "daemon" ];
              EnvironmentVariables =
                { XSP_OVERLAY = if cfg.overlay then "1" else "0"; }
                // optionalEnv "XSP_ALACRITTY" cfg.alacritty
                // optionalEnv "CODESIGN_IDENTITY" cfg.codesignIdentity
                // optionalEnv "CODESIGN_KEYCHAIN" cfg.codesignKeychain;
              LimitLoadToSessionType = "Aqua";
              RunAtLoad = true;
              KeepAlive = true;
              ProcessType = "Interactive";
              StandardErrorPath = cfg.logFile;
            };
          };
        };
    };
}
