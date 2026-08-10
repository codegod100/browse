# Browse

Minimal Radicle repo viewer: paste a `rad:z…` ID (or pass it on the CLI) and see name, description, files (README opened when present), commits, patches, issues, and job COBs for a repo already in local storage.

**Stack:** [Vidya](https://tangled.org/nandi.uk/vidya) (egui shell) · Gleam TEA guest (screens / layout opcodes) · [`radicle`](https://app.radicle.xyz) crates (Profile + storage).

## Run

```bash
nix develop          # egui libs + rust + just + cargo-apk; sets GLEAM if ~/code/gleam exists
just host
just host rad:z1ocacrfUUDHaSpspzjX5bUYf36w
just smoke rad:z1ocacrfUUDHaSpspzjX5bUYf36w

nix run . -- --smoke rad:z1ocacrfUUDHaSpspzjX5bUYf36w
nix build            # hermetic desktop binary (uses android/gleam/browse/prebuilt/browse.wasm)
```

Needs a local Radicle profile with the repo seeded. On first launch (including the Android APK), Browse shows a **Create profile** form when none exists — same as `rad auth` (alias + optional passphrase under `$RAD_HOME` / app-private storage). Live Gleam rebuilds use a wasm-capable Gleam (`~/code/gleam` on branch `wasm`); `nix build` falls back to the vendored prebuilt Wasm.

Patches / issues / jobs are a local snapshot — they do **not** auto-refresh. Press **Open** again (same RID) or re-press the active **Patches** / **Issues** / **Jobs** tab to reload from local storage.

Per-repo browser tab (Files / Commits / Patches / Issues / Jobs) and status filters are remembered in `~/.config/browse/tabs.json` (or `$XDG_CONFIG_HOME/browse/tabs.json`).

## Android APK

```bash
just android                 # nix build .#android → ./result-android/browse.apk
nix build .#android -L --out-link result-android
just install-android         # adb install -r ./result-android/browse.apk
```

Package id: `uk.nandi.browse` (aarch64 / arm64-v8a). Signed with the committed `android/ci.keystore` (password `android`, alias `androiddebugkey`).

Sibling path dep: `android/Cargo.toml` expects `../../vidya` (materialized by the flake for `nix build`, or a live checkout next to this repo).

### CI artifacts (boxci)

On merge to `main`, [boxci](https://boxci.boxd.sh) runs [`.boxci/pipeline.yml`](.boxci/pipeline.yml): clippy/check, then `nix build .#android` on nixbuild.net and publishes `browse.apk`.

```bash
./scripts/boxci/dispatch-merge.sh            # trigger merge pipeline
./scripts/boxci/dispatch-merge.sh --sha HEAD # specific SHA
```

RID: [`rad:z2QL7QdL2QGg6FmX3wcw3Mzm2ykE3`](https://nandi.radicle.garden/rad:z2QL7QdL2QGg6FmX3wcw3Mzm2ykE3). APK step soft-skips if `NIXBUILD_TOKEN` / `OPENBAO_TOKEN` are unset on the boxci host.

## View API

Gleam builds screens with view helpers in [`android/gleam/browse/src/browse.gleam`](android/gleam/browse/src/browse.gleam) (`header`, `md_body`, `file_list`, `commit_list`, …). Those pack to int opcodes; Rust [`android/src/view_api.rs`](android/src/view_api.rs) decodes them onto Vidya widgets. Dynamic repo rows live in `ViewModel` (host). Enter inventory (recent + search) is host-owned; Gleam owns enter chrome, help/about, and viewing/error copy.

### Wasm strings

When the guest is built with Gleam Wasm **strings**, it should export:

| Export | Role |
|--------|------|
| `browse__view_text(model, i) -> String` | Guest UI copy for opcode `i` |
| `browse__label(id) -> String` | Shared chrome labels (Open/RID/Files/…) |
| `memory` | Linear memory holding managed values |
| `gleam_string_utf8_len` / `gleam_string_utf8_ptr` (or `__gleam_string_*`) | Inspect a managed `String` as UTF-8 |

Host path: [`android/src/gleam_guest.rs`](android/src/gleam_guest.rs). Hermetic builds use `android/gleam/browse/prebuilt/browse.wasm` (from `tools/gen_browse_wat.py` + `browse.wat`).

## Layout

| Path | Role |
|------|------|
| `android/` | Shared lib + Android NativeActivity (`cargo-apk`) |
| `host/` | Desktop binary (`browse`) |
| `android/gleam/browse/src/browse.gleam` | TEA screens (enter/viewing/error/noprof/help/about) + labels + view DSL |
| `tools/gen_browse_wat.py` | Regenerates string-capable `android/gleam/browse/prebuilt/browse.{wat,wasm}` |
| `android/src/components/` | Meta, Readme, RepoBrowser (Files/Commits/Patches/Issues/Jobs; status tabs + search) |
| `android/src/view_api.rs` | Host paint API (`Op` / `ViewModel` → components) |
| `android/src/markdown.rs` | README via pulldown-cmark → Vidya text |
| `android/src/rad.rs` | `Profile::load` / `create_profile` + open by RID → snapshot |
| `android/src/gleam_guest.rs` | wasmtime + `browse__*` exports + String decode |
| `android/src/gleam_bridge.rs` | opcode/text fetch → `view_api` |
| `android/src/app.rs` | window, RID field, Open effect |
| `android/src/recent.rs` | recently viewed repos (`~/.config/browse/recent.json`) |
| `android/src/tab_prefs.rs` | per-repo tab + status filters (`~/.config/browse/tabs.json`) |
