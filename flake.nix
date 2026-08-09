{
  description = "Browse — Radicle repo viewer (Vidya shell + Gleam logic)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Path dep in Cargo.toml is ../vidya — pin as flake input so `nix build`
    # works without a monorepo checkout. Dev can still use the live sibling.
    vidya = {
      url = "git+https://nandi.radicle.garden/z2UqGTRH21s3pHnJgSuMwRaPPNNcW.git?ref=main";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      vidya,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

      eguiLibs =
        pkgs:
        with pkgs;
        [
          libxkbcommon
          libGL
          vulkan-loader
          openssl
        ]
        ++ lib.optionals stdenv.hostPlatform.isLinux [
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ];

      # Layout expected by Cargo.toml: parent/{browse,vidya}
      browseSrcTree =
        pkgs:
        let
          browseFiltered = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                base = baseNameOf path;
              in
              pkgs.lib.cleanSourceFilter path type
              && !(builtins.elem base [
                "target"
                "result"
                "result-browse"
                ".jj"
              ]);
          };
        in
        pkgs.runCommand "browse-src-tree" { } ''
          mkdir -p $out/{browse,vidya}
          cp -a ${browseFiltered}/. $out/browse/
          cp -a ${vidya}/. $out/vidya/
          chmod -R u+w $out
          rm -rf $out/browse/target $out/vidya/target 2>/dev/null || true
        '';
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          inherit (pkgs) lib;
          libs = eguiLibs pkgs;
          libPath = lib.makeLibraryPath libs;
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rustfmt"
              "clippy"
            ];
          };
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
          srcTree = browseSrcTree pkgs;

          browse = rustPlatform.buildRustPackage {
            pname = "browse";
            version = "0.1.0";
            src = srcTree;

            cargoRoot = "browse";
            buildAndTestSubdir = "browse";

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs = with pkgs; [
              makeWrapper
              pkg-config
            ];
            buildInputs = libs;

            OPENSSL_NO_VENDOR = "1";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            # Prefer prebuilt gleam/browse/prebuilt/browse.wasm inside the sandbox.
            GLEAM = "";
            doCheck = false;

            postInstall = ''
              wrapProgram $out/bin/browse \
                --prefix LD_LIBRARY_PATH : ${libPath}
            '';

            meta = {
              description = "Browse — Radicle repo viewer (Vidya + Gleam)";
              license = lib.licenses.mit;
              mainProgram = "browse";
            };
          };

          # Live `cargo run` launcher (edit/rebuild against checkout).
          browse-run = pkgs.writeShellApplication {
            name = "browse-run";
            text = ''
              export LD_LIBRARY_PATH="${libPath}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
              export PATH="''${HOME}/.cargo/bin:$PATH"
              if ! command -v cargo >/dev/null 2>&1; then
                for tc in \
                  "''${HOME}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin" \
                  "''${HOME}/.rustup/toolchains/stable-aarch64-unknown-linux-gnu/bin" \
                  "''${HOME}/.rustup/toolchains/"*/bin; do
                  if [ -x "''${tc}/cargo" ]; then
                    export PATH="''${tc}:$PATH"
                    break
                  fi
                done
              fi
              if ! command -v cargo >/dev/null 2>&1; then
                echo "browse-run: cargo not found — install rustup or use: nix build .#browse" >&2
                exit 127
              fi
              if [ -z "''${GLEAM:-}" ]; then
                for candidate in \
                  "''${HOME}/code/gleam/target/debug/gleam" \
                  "''${HOME}/code/gleam/target/release/gleam" \
                  "$PWD/../gleam/target/debug/gleam"; do
                  if [ -x "$candidate" ]; then
                    export GLEAM="$candidate"
                    break
                  fi
                done
              fi

              run_root() {
                local root=$1
                shift
                exec cargo run --manifest-path "$root/Cargo.toml" -- "$@"
              }

              if [ -f Cargo.toml ] && [ -d ../vidya ]; then
                run_root "$PWD" "$@"
              fi

              flake_src=${lib.escapeShellArg (toString self)}
              if [ -f "$flake_src/Cargo.toml" ]; then
                export CARGO_TARGET_DIR="''${CARGO_TARGET_DIR:-''${XDG_CACHE_HOME:-$HOME/.cache}/browse/cargo-target}"
                # Live sibling preferred; otherwise the hermetic package is .#browse.
                if [ -d "$PWD/../vidya" ]; then
                  run_root "$PWD" "$@"
                fi
                echo "browse-run: need a checkout with ../vidya, or: nix run .#browse -- …" >&2
                exit 1
              fi

              echo "browse-run: Cargo.toml not found" >&2
              exit 1
            '';
          };
        in
        {
          inherit browse;
          default = browse;
          run = browse-run;
        }
      );

      apps = forAllSystems (
        system:
        let
          browse = self.packages.${system}.browse;
          run = self.packages.${system}.run;
        in
        {
          browse = {
            type = "app";
            program = "${browse}/bin/browse";
          };
          run = {
            type = "app";
            program = "${run}/bin/browse-run";
          };
          default = self.apps.${system}.browse;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          libs = eguiLibs pkgs;
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rustfmt"
              "clippy"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.just
              pkgs.pkg-config
              pkgs.openssl
            ];
            buildInputs = libs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libs;
            OPENSSL_NO_VENDOR = "1";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
              # Prefer flake rust, then rustup toolchain bins.
              if ! command -v cargo >/dev/null 2>&1; then
                for tc in \
                  "$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin" \
                  "$HOME/.rustup/toolchains/stable-aarch64-unknown-linux-gnu/bin" \
                  "$HOME/.rustup/toolchains/"*/bin; do
                  if [ -x "''${tc}/cargo" ]; then
                    export PATH="''${tc}:$PATH"
                    break
                  fi
                done
              fi
              if [ -z "''${RUSTFLAGS:-}" ]; then
                export RUSTFLAGS="-C linker=cc -C link-arg=-fuse-ld=bfd"
              fi
              if [ -z "''${GLEAM:-}" ]; then
                for candidate in \
                  "$HOME/code/gleam/target/debug/gleam" \
                  "$HOME/code/gleam/target/release/gleam" \
                  "$PWD/../gleam/target/debug/gleam"; do
                  if [ -x "$candidate" ]; then
                    export GLEAM="$candidate"
                    break
                  fi
                done
              fi
              echo "browse — nix run | nix build | just host | just smoke <rid>''${GLEAM:+ (GLEAM=$GLEAM)}"
            '';
          };
        }
      );
    };
}
