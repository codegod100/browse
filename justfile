# Browse — desktop host + Android APK (use inside `nix develop` or with rustup)

set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Desktop window
host rid='':
    #!/usr/bin/env bash
    set -euo pipefail
    export GLEAM="${GLEAM:-${HOME}/code/gleam/target/debug/gleam}"
    if [[ -n "{{rid}}" ]]; then
      cargo run --manifest-path host/Cargo.toml -- "{{rid}}"
    else
      cargo run --manifest-path host/Cargo.toml
    fi

smoke rid:
    #!/usr/bin/env bash
    set -euo pipefail
    export GLEAM="${GLEAM:-${HOME}/code/gleam/target/debug/gleam}"
    cargo run --manifest-path host/Cargo.toml -- --smoke "{{rid}}"

# Check / build the Android package as a library (desktop target)
lib:
    cargo build --manifest-path android/Cargo.toml --lib

# Phone APK (aarch64) via flake
android:
    nix build .#android -L --out-link result-android

# adb install result of .#android
install-android:
    #!/usr/bin/env bash
    set -euo pipefail
    APK="./result-android/browse.apk"
    [[ -f "$APK" ]] || { echo "missing $APK — run: just android" >&2; exit 1; }
    adb install -r "$APK"
