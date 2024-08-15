{
  description = "LSP-AI - An open-source language server that serves as a backend for AI-powered functionality, designed to assist and empower software engineers, not replace them.";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs.url = "github:nixos/nixpkgs/release-24.05";

    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.11.0";
    cargo2nix.inputs.nixpkgs.follows = "nixpkgs";
    cargo2nix.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { cargo2nix, flake-utils, nixpkgs, ... }:
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
