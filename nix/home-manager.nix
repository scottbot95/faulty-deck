{ config, ... }:
let
  mkDaemon = system: (config.perSystem system).packages.daemon;
in
{
  flake.homeModules.default = 
    { config, lib, pkgs, ... }:
    let
      inherit (lib) mkOption mkEnableOption mkIf types;
      cfg = config.services.faultydeck;
      tomlFormat = pkgs.formats.toml { };

      daemon = mkDaemon pkgs.stdenv.hostPlatform.system;

      settingsType = types.submodule {
        freeformType = tomlFormat.type;
      };

      configFile = tomlFormat.generate "faultydeck.toml"
        (lib.filterAttrsRecursive (_: v: v != null) cfg.settings);
    in
    {
      options.services.faultydeck = {
        enable = mkEnableOption "FaultyDeck daemon";
        package = mkOption {
          type = types.package;
          default = daemon;
        };
        settings = mkOption {
          type = settingsType;
          default = { };
          description = "Contents of ~/.config/faultydeck/config.toml";
        };
      };

      config = mkIf cfg.enable {
        xdg.configFile."faultydeck/config.toml".source = configFile;

        systemd.user.services.faultydeck = {
          Unit = {
            Description = "FaultyDeck daemon";
            After = [ "graphical-session.target" "pipewire-pulse.service" ];
            PartOf = [ "graphical-session.target" ];
            X-Restart-Triggers = [ configFile ];
          };
          Service = {
            ExecStart = "${cfg.package}/bin/daemon --config-file %E/faultydeck/config.toml";
            Restart = "on-failure";
          };
          Install.WantedBy = [ "graphical-session.target" ];
        };
      };
    };
}
