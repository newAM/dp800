{
  description = "Rigol DP832 TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
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

      src = ./.;

      cargoArtifacts = craneLib.buildDepsOnly {inherit src;};
    in rec {
      packages.default = craneLib.buildPackage {
        inherit src;
        inherit cargoArtifacts;
      };

      apps.default = {
        type = "app";
        program = "${packages.default}/bin/dp832";
      };

      checks = {
        pkg = packages.default;

        clippy = craneLib.cargoClippy {
          inherit cargoArtifacts src;
          cargoClippyExtraArgs = "-- --deny warnings";
        };

        rustfmt = craneLib.cargoFmt {
          inherit cargoArtifacts src;
        };

        alejandra = pkgs.runCommand "alejandra" {} ''
          ${pkgs.alejandra}/bin/alejandra --check ${src}
          touch $out
        '';

        statix = pkgs.runCommand "statix" {} ''
          ${pkgs.statix}/bin/statix check ${src}
          touch $out
        '';
      };
    });
}
