{ inputs, ... }:
{
  imports = [
    inputs.rust-flake.flakeModules.default
    inputs.rust-flake.flakeModules.nixpkgs
  ];

  perSystem =
    {
      config,
      self',
      pkgs,
      lib,
      ...
    }:
    let
      craneLib = config.rust-project.crane-lib;
    in
    {
      rust-project = {
        # Only include the native linux crates (eg no `firmware/`)
        src = lib.fileset.toSource {
          root = ../.;
          fileset = lib.fileset.unions [
            ../Cargo.toml
            ../Cargo.lock
            (craneLib.fileset.commonCargoSources ../crates)
          ];
        };

        crates."daemon".crane.args = {
          buildInputs = [
            pkgs.libpulseaudio
          ];
        };
      };

      packages.default = self'.packages.daemon;

      devShells = {
        default = craneLib.devShell {
          checks = self'.checks;
          packages = [
            pkgs.just
            pkgs.rust-analyzer
          ];
        };

        firmware = pkgs.mkShell {
          packages = [
            # Build toolchain
            pkgs.rustup
            pkgs.espup
            pkgs.just
            pkgs.pkg-config

            # Flashing tools
            pkgs.espflash
            pkgs.probe-rs-tools
          ];
        };
      };

      formatter = pkgs.nixfmt-tree;
    };
}
