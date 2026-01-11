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

        # Use fenix for the Rust toolchain (with llvm-tools for coverage)
        toolchain = fenix.packages.${system}.stable.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rustc"
          "rustfmt"
          "llvm-tools"
        ];

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

          buildInputs =
            [] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
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

        # Common devShell configuration factory
        mkDevShell = { extraShellHook ? "" }: craneLib.devShell {
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
            cargo-llvm-cov

            # Build dependencies
            pkg-config

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
            ${extraShellHook}
          '';
        };

      in
      {
        checks = {
          # Build the crate as part of `nix flake check`
          inherit ralph;

          # Run clippy
          # Allow some pedantic warnings (rust-2024-compatibility issues are handled via #[allow] attributes)
          ralph-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings --allow clippy::unused-async --allow clippy::similar-names";
          });

          # Check formatting
          ralph-fmt = craneLib.cargoFmt {
            inherit src;
          };

          # Run tests
          ralph-test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            # Add git for tests that need it
            buildInputs = commonArgs.buildInputs ++ [ pkgs.git ];
          });

          # Coverage is checked via `cargo llvm-cov` (see AGENTS.md)
          # Not in nix checks due to rebuild overhead
        };

        packages = {
          default = ralph;
          inherit ralph;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = ralph;
          };

          # Coverage check app - run with: nix run .#coverage
          coverage = flake-utils.lib.mkApp {
            drv = pkgs.writeShellApplication {
              name = "ralph-coverage";
              runtimeInputs = [
                toolchain
                pkgs.cargo-llvm-cov
                pkgs.pkg-config
                pkgs.openssl
              ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.darwin.apple_sdk.frameworks.Security
                pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
                pkgs.libiconv
              ];
              text = ''
                echo "Running coverage check (75% threshold)..."
                cargo llvm-cov --release --fail-under-lines 75 --ignore-filename-regex '(nix/store|\.cargo/|main\.rs$|rustlib|sandbox/docker\.rs)' "$@"
              '';
            };
          };
        };

        devShells = {
          # Default shell for human development
          default = mkDevShell { };

          # Agent shell for Ralph worktrees (configures signing identity)
          agent = mkDevShell {
            extraShellHook = ''
              # Configure Ralph's signing identity for automated commits
              AGENT_SIGNING_KEY="$HOME/.ssh/ralph_signing"
              if [ -f "$AGENT_SIGNING_KEY" ]; then
                git config --local user.name "Agent"
                git config --local user.email "agent@localhost"
                git config --local gpg.format ssh
                git config --local user.signingkey "$AGENT_SIGNING_KEY"
                git config --local commit.gpgsign true
                echo "🤖 Ralph signing identity configured"
              else
                echo "⚠️  Ralph signing key not found at $AGENT_SIGNING_KEY"
                echo "   Generate with: ssh-keygen -t ed25519 -f $AGENT_SIGNING_KEY -N \"\" -C \"ralph-agent\""
              fi
              echo ""
            '';
          };
        };
      });
}
