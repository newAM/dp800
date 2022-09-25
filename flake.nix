{
  description = "Rigol DP832 TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
  }:
    flake-utils.lib.eachSystem ["x86_64-linux"] (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.lib.${system};

      commonArgs = {
        src = ./.;
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in rec {
      packages.default = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
        });

      apps.default = flake-utils.lib.mkApp {drv = packages.default;};

      checks = {
        pkg = packages.default;

        clippy = craneLib.cargoClippy (commonArgs
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "-- --deny warnings";
          });

        rustfmt = craneLib.cargoFmt {src = ./.;};

        alejandra = pkgs.runCommand "alejandra" {} ''
          ${pkgs.alejandra}/bin/alejandra --check ${./.}
          touch $out
        '';

        statix = pkgs.runCommand "statix" {} ''
          ${pkgs.statix}/bin/statix check ${./.}
          touch $out
        '';
      };
    });
}
