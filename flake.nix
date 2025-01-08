{
  description = "LSP-AI: Open-source language server for AI-powered functionality";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };
        rustVersion = pkgs.rust-bin.stable.latest.default;

        commonBuildInputs = with pkgs; [
          openssl
          zlib
        ];

        commonNativeBuildInputs = with pkgs; [
          pkg-config
          cmake
          rustVersion
          perl
        ];

      in
      rec {
        packages = rec {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "lsp-ai";
            version = "0.7.0";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs = commonNativeBuildInputs;
            buildInputs = commonBuildInputs;

            doCheck = false;

            meta = with pkgs.lib; {
              description = "An open-source language server that serves as a backend for AI-powered functionality";
              homepage = "https://github.com/SilasMarvin/lsp-ai";
              license = licenses.mit;
              maintainers =
                [
                  # Add maintainers here
                ];
            };
          };

          devPackage = default.overrideAttrs (oldAttrs: {
            name = "lsp-ai-dev";

            buildPhase = ''
              export CARGO_HOME="/tmp/.cargo"
              export RUSTUP_HOME="/tmp/.rustup"
              cargo build
            '';

            installPhase = ''
              mkdir -p $out/bin
              cp target/debug/lsp-ai $out/bin/
            '';
          });
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ packages.default ];
          packages = with pkgs; [
            rust-analyzer
            clippy
            rustfmt
          ];

          shellHook = ''
            export CARGO_HOME="/tmp/.cargo"
            export RUSTUP_HOME="/tmp/.rustup"
          '';
        };
      }
    );
}
