{
  description = "Rigol DP832 TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    crane.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
  }:
    flake-utils.lib.eachSystem [
      "aarch64-darwin"
      "aarch64-linux"
      "x86_64-linux"
    ] (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.lib.${system};

      src = craneLib.cleanCargoSource ./.;
      buildInputs = nixpkgs.lib.optional pkgs.stdenv.isDarwin pkgs.libiconv;

      cargoArtifacts = craneLib.buildDepsOnly {
        inherit src buildInputs;
      };
    in {
      packages.default = craneLib.buildPackage {
        inherit cargoArtifacts src buildInputs;
      };

      apps.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/dp832";
      };

      checks = let
        nixSrc = nixpkgs.lib.sources.sourceFilesBySuffices ./. [".nix"];
      in {
        pkg = self.packages.${system}.default;

        clippy = craneLib.cargoClippy {
          inherit cargoArtifacts src;
          cargoClippyExtraArgs = "-- --deny warnings";
        };

        rustfmt = craneLib.cargoFmt {
          inherit cargoArtifacts src;
        };

        alejandra = pkgs.runCommand "alejandra" {} ''
          ${pkgs.alejandra}/bin/alejandra --check ${nixSrc}
          touch $out
        '';

        statix = pkgs.runCommand "statix" {} ''
          ${pkgs.statix}/bin/statix check ${nixSrc}
          touch $out
        '';
      };
    });
}
