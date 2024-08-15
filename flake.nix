{
  description = "LSP-AI - An open-source language server that serves as a backend for AI-powered functionality, designed to assist and empower software engineers, not replace them.";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs.url = "github:nixos/nixpkgs/release-24.05";

    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.11.0";
    cargo2nix.inputs.nixpkgs.follows = "nixpkgs";
    cargo2nix.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, flake-utils, nixpkgs, ... } @inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ inputs.cargo2nix.overlays.default ];
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

              inputs.cargo2nix.packages."${system}".default
            ];
          };
        };

        packages = rec {
          default = lsp-ai;

          lsp-ai = (rustPkgs.workspace.lsp-ai { });
        };

        nixosModules = rec {
          default = lsp-ai;

          lsp-ai = { pkgs, lib, config, ... }:
            with lib;
            let
              cfg = config.programs.lsp-ai;
            in
            {
              options.programs.lsp-ai = {
                enable = mkEnableOption (mdDoc "lsp-ai");
              };

              config = mkIf cfg.enable {
                environment.systemPackages = [
                  self.packages.${pkgs.system}.default
                ];
              };
            };
        };

        homeManagerModules = rec {
          default = lsp-ai;

          lsp-ai = { pkgs, lib, config, ... }:
            with lib;
            let
              cfg = config.programs.lsp-ai;
            in
            {
              options.programs.lsp-ai = {
                enable = mkEnableOption (mdDoc "lsp-ai");
              };

              config = mkIf cfg.enable {
                home.packages = [
                  self.packages.${pkgs.system}.default
                ];
              };
            };
        };
      });
}
