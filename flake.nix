{
  description = "Rigol DP832 TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    advisory-db.url = "github:rustsec/advisory-db";
    advisory-db.flake = false;

    treefmt.url = "github:numtide/treefmt-nix";
    treefmt.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    treefmt,
    advisory-db,
  }: let
    forEachSystem = nixpkgs.lib.genAttrs [
      "aarch64-darwin"
      "aarch64-linux"
      "x86_64-darwin"
      "x86_64-linux"
    ];

    workspaceToml = nixpkgs.lib.importTOML ./Cargo.toml;
    cargoToml = nixpkgs.lib.importTOML ./dp832/Cargo.toml;

    # https://github.com/ipetkov/crane/blob/112e6591b2d6313b1bd05a80a754a8ee42432a7e/lib/cleanCargoSource.nix
    cargoSrc = nixpkgs.lib.cleanSourceWith {
      # Apply the default source cleaning from nixpkgs
      src = nixpkgs.lib.cleanSource self;
      # https://github.com/ipetkov/crane/blob/112e6591b2d6313b1bd05a80a754a8ee42432a7e/lib/filterCargoSources.nix
      filter = orig_path: type: let
        path = toString orig_path;
        base = baseNameOf path;
        parentDir = baseNameOf (dirOf path);

        matchesSuffix = nixpkgs.lib.any (suffix: nixpkgs.lib.hasSuffix suffix base) [
          # Keep rust sources
          ".rs"
          # Keep all toml files as they are commonly used to configure other
          # cargo-based tools
          ".toml"
        ];

        # Cargo.toml already captured above
        isCargoFile = base == "Cargo.lock";

        # .cargo/config.toml already captured above
        isCargoConfig = parentDir == ".cargo" && base == "config";
      in
        type == "directory" || matchesSuffix || isCargoFile || isCargoConfig;
    };

    treefmtEval = pkgs:
      treefmt.lib.evalModule pkgs {
        projectRootFile = "flake.nix";
        programs = {
          alejandra.enable = true;
          prettier.enable = true;
          rustfmt.enable = true;
          taplo.enable = true;
        };
      };

    overlay = final: prev: {
      dp832 = prev.rustPlatform.buildRustPackage {
        pname = cargoToml.package.name;
        version = workspaceToml.workspace.package.version;

        src = cargoSrc;

        cargoDeps = prev.rustPlatform.importCargoLock {
          lockFile = ./Cargo.lock;
        };

        nativeCheckInputs = [
          prev.cargo-audit
          prev.clippy
        ];

        preCheck = ''
          echo "Running cargo audit..."
          cargo audit -n -d ${advisory-db} --ignore yanked

          echo "Running clippy..."
          cargo clippy -- -Dwarnings
        '';

        meta = {
          description = cargoToml.package.description;
          homepage = cargoToml.workspace.package.repository;
          license = prev.lib.licenses.mit;
          maintainers = [prev.lib.maintainers.newam];
        };
      };
    };
  in {
    packages = forEachSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [overlay];
        };
      in {
        default = pkgs.dp832;
      }
    );

    apps = forEachSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [overlay];
        };
      in {
        default = {
          type = "app";
          program = "${pkgs.dp832}/bin/dp832";
          inherit (pkgs.dp832) meta;
        };
      }
    );

    formatter = forEachSystem (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
      in
        (treefmtEval pkgs).config.build.wrapper
    );

    checks = forEachSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [overlay];
      };
    in {
      dp832 = pkgs.dp832;

      formatting = (treefmtEval pkgs).config.build.check self;
    });

    overlays.default = overlay;
  };
}
