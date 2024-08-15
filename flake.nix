let
  cargo_toml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
in
{
  description = cargo_toml.description;

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs.url = "github:nixos/nixpkgs/release-24.05";

    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.11.0";
    cargo2nix.inputs.nixpkgs.follows = "nixpkgs";
    cargo2nix.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, cargo2nix, flake-utils, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ cargo2nix.overlays.default ];
        };

        rustPkgs = pkgs.rustBuilder.makePackageSet {
          rustVersion = "1.75.0";
          packageFun = import ./Cargo.nix;
        };

      in
      {
        devShells = rec {
          default = dev;

          dev = rustPkgs.workspaceShell {
            packages = with pkgs; [
              # nix
              nil
              nixpkgs-fmt

              # rust
            ];
          };
        };

        packages = rec {
          default = lsp-ai;

          lsp-ai = (rustPkgs.workspace.lsp-ai { });
        };
      }
    );
}
