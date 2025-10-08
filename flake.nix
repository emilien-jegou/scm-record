{
  description = "A flake for tug-record, a Rust-based SCM interaction recorder.";

  # Flake inputs: dependencies for our flake
  inputs = {
    # Nixpkgs: the source of all packages
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";

    # Flake-utils: a helper for creating flakes that work on multiple systems
    flake-utils.url = "github:numtide/flake-utils";

    # Rust-overlay: provides up-to-date Rust toolchains
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  # Flake outputs: what this flake provides (packages, shells, etc.)
  outputs = { self, nixpkgs, flake-utils, rust-overlay }@inputs:
    # Use flake-utils to generate outputs for common systems
    flake-utils.lib.eachDefaultSystem (system:
      let
        # Import nixpkgs for the given system and apply the rust-overlay
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # --- RUST TOOLCHAIN CONFIGURATION ---
        rustToolchain = pkgs.rust-bin.nightly."2025-09-01".default.override {
          # Extensions are needed for the dev shell (rust-analyzer, etc.)
          extensions = [ "rust-src" "rustfmt" "clippy" "rust-analyzer" ];
          # You can add targets here if needed for cross-compilation
          # targets = ["wasm32-unknown-unknown"];
        };

      in
      {
        # --- PACKAGES ---
        # This section defines the buildable artifacts from your project.
        packages = {
          # This is the main package, named 'tug-record' as requested.
          tug-record = pkgs.rustPlatform.buildRustPackage {
            pname = "tug-record";
            version = "0.1.0"; # Tip: You can parse this from Cargo.toml

            # The source code is the flake's own directory
            src = self;

            # This ensures a reproducible build using the lockfile
            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            # Inject our specific rust toolchain into the build platform
            # This is crucial for using a specific nightly version.
            rustc = rustToolchain;

            # System dependencies needed to build the project.
            # These were commented out in your shell.nix, but are common for Rust projects.
            # Add or remove them as needed.
            buildInputs = with pkgs; [
              openssl
              pkg-config
              wmctrl # Add any other system dependencies here
            ];

            # For Rust projects with native dependencies, you often need this.
            # It makes tools like `pkg-config` available to cargo.
            nativeBuildInputs = with pkgs; [ pkg-config ];

            # Optional: Add metadata for `nix search` and other tools
            meta = with pkgs.lib; {
              description = "A tool to record and replay SCM interactions.";
              homepage = "https://github.com/emilien-jegou/scm-record";
              license = licenses.mit;
              mainProgram = "tug";
            };
          };

          # The 'default' package is a standard alias for the main package.
          # This allows users to just run `nix build` without specifying the package name.
          default = self.packages.${system}.tug-record;
        };

        # --- DEVELOPMENT SHELL ---
        # This provides a reproducible development environment.
        devShells.default = pkgs.mkShell {
          # Build inputs are dependencies available inside the shell
          buildInputs = with pkgs; [
            rustToolchain
            openssl
            pkg-config
            wmctrl
          ];

          # This hook runs when you enter the shell
          shellHook = ''
            [ ! -f .packages/bin/bacon ] && cargo install bacon --locked --root .packages/

            export PATH="$PATH:$(pwd)/.packages/bin/:$(pwd)/bin/";

            [ -f .localrc ] && source .localrc
          '';
        };

      });
}
