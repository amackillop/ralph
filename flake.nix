{
  description = "ralph - Ralph Wiggum technique for iterative AI development";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Use fenix for the Rust toolchain
        toolchain = fenix.packages.${system}.stable.toolchain;

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        # Common arguments for crane builds
        # Use a custom filter that includes template files (.md, .mdc, .toml in src/templates)
        srcFilter = path: type:
          (craneLib.filterCargoSources path type) ||
          (builtins.match ".*\.md$" path != null) ||
          (builtins.match ".*\.mdc$" path != null) ||
          (builtins.match ".*src/templates/.*\.toml$" path != null);

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = srcFilter;
        };

        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = [
            pkgs.openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            pkgs.libiconv
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
          ];
        };

        # Build just the cargo dependencies for caching
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate
        ralph = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;

          # Disable tests during build (run separately)
          doCheck = false;
        });

      in
      {
        checks = {
          # Build the crate as part of `nix flake check`
          inherit ralph;

          # Run clippy
          ralph-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          # Check formatting
          ralph-fmt = craneLib.cargoFmt {
            inherit src;
          };

          # Run tests
          ralph-test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        packages = {
          default = ralph;
          inherit ralph;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = ralph;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks
          checks = self.checks.${system};

          # Additional dev tools
          packages = with pkgs; [
            # Rust tools (from fenix toolchain)
            rust-analyzer
            cargo-watch
            cargo-edit
            cargo-audit
            cargo-outdated

            # Build dependencies
            pkg-config
            openssl

            # Docker for sandbox functionality
            docker
            docker-compose

            # Git for version control
            git

            # Useful utilities
            jq
            just
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.SystemConfiguration
            libiconv
          ];

          # Environment variables
          shellHook = ''
            echo "ralph development shell"
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build    - Build the project"
            echo "  cargo test     - Run tests"
            echo "  cargo run      - Run the CLI"
            echo "  cargo watch    - Watch for changes"
            echo ""
          '';
        };
      });
}
