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
    nixpkgs.lib.recursiveUpdate
    (flake-utils.lib.eachSystem [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ] (system: let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.lib.${system};
        repo = builtins.path {
          path = ./.;
          name = "dp800-source";
        };
        cargoToml = nixpkgs.lib.importTOML ./dp832/Cargo.toml;
        inherit (cargoToml.package) version;
        pname = cargoToml.package.name;

        src = craneLib.cleanCargoSource repo;
        buildInputs = nixpkgs.lib.optional pkgs.stdenv.isDarwin pkgs.libiconv;

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src buildInputs pname version;
        };
      in {
        packages.default = craneLib.buildPackage {
          inherit cargoArtifacts src buildInputs pname version;
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/dp832";
        };

        checks = let
          nixSrc = nixpkgs.lib.sources.sourceFilesBySuffices repo [".nix"];
        in {
          pkg = self.packages.${system}.default;

          clippy = craneLib.cargoClippy {
            inherit cargoArtifacts src pname version;
            cargoClippyExtraArgs = "-- --deny warnings";
          };

          rustfmt = craneLib.cargoFmt {
            inherit cargoArtifacts src pname version;
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
      })) {
      overlays.default = final: prev: {
        dp832 = self.packages.${prev.system}.default;
      };
    };
}
