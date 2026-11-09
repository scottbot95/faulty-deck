{ ... }:
{
  flake.nixosModules.default = 
    { config, lib, pkgs, ... }:
    let
      cfg = config.services.faultydeck;
    in {
      options.services.faultydeck.enable = lib.mkEnableOption "FaultyDeck daemon udev rules";

      config = lib.mkIf cfg.enable {
        services.udev.packages = [
          (pkgs.writeTextDir "lib/udev/rules.d/69-faulty-deck.rules" ''
            SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="0001", TAG+="uaccess"
          '')
        ];
      };
    };
}
