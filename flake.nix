{
  description = "Browse — Radicle repo viewer (Vidya shell + Gleam logic)";

  nixConfig = {
    extra-substituters = [ "https://codegod100.cachix.org" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Path deps: android → ../../vidya, host → ../android.
    # Pin vidya so `nix build` works without a monorepo checkout.
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
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
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

      # Layout expected by android/Cargo.toml path deps:
      #   parent/browse/{android,host}
      #   parent/vidya
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
                "result-android"
                ".jj"
                ".github"
              ]);
          };
        in
        pkgs.runCommand "browse-src-tree" { } ''
          mkdir -p $out/{browse,vidya}
          cp -a ${browseFiltered}/. $out/browse/
          cp -a ${vidya}/. $out/vidya/
          chmod -R u+w $out
          # Older tips of vidya referenced winit on Android without declaring it.
          if [ -f ${./patches/vidya-android-winit.patch} ]; then
            patch -p1 -d $out/vidya --forward --batch \
              < ${./patches/vidya-android-winit.patch} || true
          fi
          rm -rf $out/browse/{.git,host/target,android/target} 2>/dev/null || true
        '';

      androidApiLevel = "28";
      androidTarget = "aarch64-linux-android";
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
          rustAndroid = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rustfmt"
              "clippy"
            ];
            targets = [
              androidTarget
              "x86_64-linux-android"
            ];
          };
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
          rustPlatformAndroid = pkgs.makeRustPlatform {
            cargo = rustAndroid;
            rustc = rustAndroid;
          };
          srcTree = browseSrcTree pkgs;

          browse = rustPlatform.buildRustPackage {
            pname = "browse";
            version = "0.1.0";
            src = srcTree;

            cargoRoot = "browse/host";
            buildAndTestSubdir = "browse/host";

            cargoLock = {
              lockFile = ./host/Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs = with pkgs; [
              makeWrapper
              pkg-config
            ];
            buildInputs = libs;

            OPENSSL_NO_VENDOR = "1";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            # Prefer prebuilt android/gleam/browse/prebuilt/browse.wasm.
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

          androidComposition = pkgs.androidenv.composeAndroidPackages {
            platformVersions = [ "34" ];
            buildToolsVersions = [ "34.0.0" ];
            includeNDK = true;
            includeEmulator = false;
            includeSystemImages = false;
          };

          androidSdk = androidComposition.androidsdk;
          androidSdkRoot = "${androidSdk}/libexec/android-sdk";

          browse-android = pkgs.stdenv.mkDerivation {
            pname = "browse-android";
            version = "0.1.0";
            src = srcTree;

            cargoRoot = "browse/android";
            cargoDeps = rustPlatformAndroid.importCargoLock {
              lockFile = ./android/Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs = [
              rustAndroid
              pkgs.cargo-apk
              pkgs.jdk17_headless
              pkgs.perl
              pkgs.gnumake
              pkgs.cmake
              rustPlatformAndroid.cargoSetupHook
            ];

            strictDeps = true;
            dontUseCmakeConfigure = true;
            disallowedReferences = [ rustAndroid ];

            ANDROID_HOME = androidSdkRoot;
            ANDROID_SDK_ROOT = androidSdkRoot;
            ANDROID_NDK_HOME = "${androidSdkRoot}/ndk-bundle";
            ANDROID_NDK_ROOT = "${androidSdkRoot}/ndk-bundle";

            buildPhase = ''
              runHook preBuild

              export HOME="$TMPDIR/home"
              mkdir -p "$HOME/.android"

              keystore="$(pwd)/browse/android/ci.keystore"
              [[ -f "$keystore" ]] || {
                echo "missing CI keystore at $keystore" >&2
                exit 1
              }

              ndk="$ANDROID_NDK_HOME"
              if [[ ! -d "$ndk" ]]; then
                ndk="$(echo "$ANDROID_HOME"/ndk/* | awk '{print $1}')"
                export ANDROID_NDK_HOME="$ndk"
                export ANDROID_NDK_ROOT="$ndk"
              fi
              [[ -d "$ndk" ]] || {
                echo "Android NDK not found under $ANDROID_HOME" >&2
                ls -la "$ANDROID_HOME" >&2 || true
                exit 1
              }

              prebuilt=""
              for host in linux-x86_64 linux-aarch64; do
                if [[ -d "$ndk/toolchains/llvm/prebuilt/$host/bin" ]]; then
                  prebuilt="$ndk/toolchains/llvm/prebuilt/$host/bin"
                  break
                fi
              done
              [[ -n "$prebuilt" ]] || {
                echo "NDK llvm prebuilt toolchain not found under $ndk" >&2
                exit 1
              }
              export PATH="$prebuilt:$PATH"

              export CC_aarch64_linux_android="aarch64-linux-android${androidApiLevel}-clang"
              export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
              export AR_aarch64_linux_android=llvm-ar
              export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR=llvm-ar
              # Vendored OpenSSL for radicle/git2 on Android.
              export OPENSSL_DIR=""
              unset OPENSSL_NO_VENDOR || true

              echo "cargo apk build --release --target ${androidTarget} -p browse" >&2
              echo "  ANDROID_HOME=$ANDROID_HOME" >&2
              echo "  ANDROID_NDK_HOME=$ANDROID_NDK_HOME" >&2
              echo "  linker=$CC_aarch64_linux_android" >&2

              pushd browse/android >/dev/null
              # Cargo.toml already points at ci.keystore (relative); ensure it exists.
              [[ -f ci.keystore ]] || cp "$keystore" ci.keystore
              cargo apk build --release --target ${androidTarget} -p browse --lib
              popd >/dev/null

              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p $out
              apk=""
              for cand in \
                browse/android/target/release/apk/browse.apk \
                browse/android/target/browse.apk \
                browse/android/target/release/apk/browse-release.apk; do
                if [[ -f "$cand" ]]; then
                  apk="$cand"
                  break
                fi
              done
              if [[ -z "''${apk:-}" ]]; then
                apk="$(find browse/android/target -type f -path '*/release/apk/*.apk' ! -name '*-unaligned.apk' 2>/dev/null | head -1 || true)"
              fi
              [[ -n "''${apk:-}" && -f "$apk" ]] || {
                echo "APK not found under browse/android/target" >&2
                find browse/android/target -name '*.apk' -o -name '*.so' 2>/dev/null | head -50 >&2 || true
                exit 1
              }
              cp "$apk" $out/browse.apk
              apksigner="$(echo "$ANDROID_HOME"/build-tools/*/apksigner | awk '{print $NF}')"
              "$apksigner" verify --verbose "$out/browse.apk"
              "$apksigner" verify --print-certs "$out/browse.apk" | grep -q 'CN=Browse CI'
              ln -s browse.apk $out/app.apk
              cat > $out/metadata.txt <<EOF
              package=uk.nandi.browse
              activity=android.app.NativeActivity
              target=${androidTarget}
              profile=release
              EOF
              runHook postInstall
            '';

            meta = with pkgs.lib; {
              description = "Browse — Android APK (aarch64 / arm64-v8a)";
              license = licenses.mit;
              platforms = platforms.linux;
            };
          };

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

              if [ -f host/Cargo.toml ] && [ -d ../vidya -o -d /vidya ]; then
                exec cargo run --manifest-path host/Cargo.toml -- "$@"
              fi

              echo "browse-run: need host/Cargo.toml with ../../vidya (or /vidya), or: nix run .#browse -- …" >&2
              exit 1
            '';
          };
        in
        {
          inherit browse;
          default = browse;
          run = browse-run;
          android = browse-android;
          inherit browse-android;
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
            targets = [
              androidTarget
              "x86_64-linux-android"
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
              pkgs.cargo-apk
              pkgs.android-tools
              pkgs.jdk17_headless
            ];
            buildInputs = libs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libs;
            OPENSSL_NO_VENDOR = "1";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
              if [ -z "''${RUSTFLAGS:-}" ]; then
                export RUSTFLAGS="-C linker=cc -C link-arg=-fuse-ld=bfd"
              fi
              echo "browse — nix build .#browse | nix build .#android | just host | just android"
            '';
          };
        }
      );
    };
}
