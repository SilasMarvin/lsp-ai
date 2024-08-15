{
  description = "LSP-AI - The official VS Code plugin for LSP-AI. LSP-AI is an open-source language server that serves as a backend for AI-powered functionality, designed to assist and empower software engineers, not replace them.";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs.url = "github:nixos/nixpkgs/release-24.05";

    node2nix.url = "github:svanderburg/node2nix";
    node2nix.inputs.nixpkgs.follows = "nixpkgs";
    node2nix.inputs.flake-utils.follows = "flake-utils";
  };

  outputs = { self, node2nix, flake-utils, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ node2nix.overlays."${system}" ];
        };

        node-env = import ./default.nix {
          inherit pkgs system;
          nodejs = pkgs.nodejs;
        };
      in
      {
        devShells = rec {
          default = dev;
          dev = node-env.shell;
        };

        packages = rec {
          default = lsp-ai;
          lsp-ai = node-env.package;
        };
      });
}
