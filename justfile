# Browse — desktop host (use inside `nix develop` or with rustup)

host rid='':
    #!/usr/bin/env bash
    set -euo pipefail
    export GLEAM="${GLEAM:-${HOME}/code/gleam/target/debug/gleam}"
    if [[ -n "{{rid}}" ]]; then
      cargo run -- "{{rid}}"
    else
      cargo run
    fi

smoke rid:
    #!/usr/bin/env bash
    set -euo pipefail
    export GLEAM="${GLEAM:-${HOME}/code/gleam/target/debug/gleam}"
    cargo run -- --smoke "{{rid}}"
