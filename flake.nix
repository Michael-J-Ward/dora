{

  # nix.conf = {
  #   substituters = "https://cache.nixos.org https://ros.cachix.org";
  #   trusted-public-keys = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= ros.cachix.org-1:dSyZxI8geDCJrwgvCOHDoAfOm5sV1wCPjBkKL+38Rvo=";
  # };
  # template: https://github.com/srid/rust-nix-template/blob/master/flake.nix
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    # nix-ros-overlay = {
    #   url = "github:lopsided98/nix-ros-overlay";
    #   inputs.nixpkgs.follows = "nixpkgs";
    # };

    # Dev tools
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.treefmt-nix.flakeModule
      ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        let
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          qt-package = pkgs.qt5.qtbase;
          nonRustDeps = [
            pkgs.libiconv
            pkgs.pkg-config

            pkgs.stdenv.cc.cc
            pkgs.zlib

            pkgs.mesa
            pkgs.libGL
            pkgs.libGL.dev
            pkgs.libglvnd
            pkgs.libglvnd.dev
            pkgs.glib

            pkgs.xorg.libX11
            pkgs.xorg.libXext
            pkgs.xorg.libSM
            pkgs.xorg.libICE
            pkgs.xorg.libxcb.dev



            pkgs.qtcreator
            qt-package
            # pkgs.libxcb
            
          ];
          rust-toolchain = pkgs.symlinkJoin {
            name = "rust-toolchain";
            paths = [
              pkgs.rustc
              pkgs.cargo
              pkgs.clippy
              pkgs.cargo-watch
              pkgs.rust-analyzer
              pkgs.rustPlatform.rustcSrc
              (pkgs.python3.withPackages (python-pkgs: [
                python-pkgs.numpy
                python-pkgs.pyarrow
              ]))
              pkgs.maturin
            ];
          };
          QT_QPA_PLATFORM_PLUGIN_PATH="${qt-package.bin}/lib/qt-${qt-package.version}/plugins/platforms";
          NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
              pkgs.stdenv.cc.cc
              pkgs.zlib
              
              # TODO: unsure if all of these are necessary
              pkgs.mesa
              pkgs.libGL
              pkgs.libGL.dev
              pkgs.libglvnd
              pkgs.libglvnd.dev
              pkgs.glib
              
              pkgs.xorg.libX11
              pkgs.xorg.libXext
              pkgs.xorg.libSM
              pkgs.xorg.libICE
              pkgs.xorg.libxcb.dev

              pkgs.qtcreator
              qt-package

            ];
        in
        {
          # Rust package
          packages.default = pkgs.rustPlatform.buildRustPackage {
            inherit (cargoToml.package) name version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
          };

          # Rust dev environment
          devShells.default = pkgs.mkShell {
            inherit NIX_LD_LIBRARY_PATH QT_QPA_PLATFORM_PLUGIN_PATH;
            inputsFrom = [
              config.treefmt.build.devShell
            ];
            shellHook = ''
              # For rust-analyzer 'hover' tooltips to work.
              export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}

              echo
              echo "🍎🍎 Run 'just <recipe>' to get started"
              just
            '';
            buildInputs = nonRustDeps;
            nativeBuildInputs = with pkgs; [
              just
              rust-toolchain
              (pkgs.hiPrio pkgs.bashInteractive) # needed so it doesn't mangle terminal in vscode
            ];
            # RUST_BACKTRACE = 1;
          };

          # Add your auto-formatters here.
          # cf. https://numtide.github.io/treefmt/
          treefmt.config = {
            projectRootFile = "flake.nix";
            programs = {
              nixpkgs-fmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };
    };
}
