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

Needs a local Radicle profile with the repo seeded. Live Gleam rebuilds use a wasm-capable Gleam (`~/code/gleam` on branch `wasm`); `nix build` falls back to the vendored prebuilt Wasm.

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

Gleam builds screens with view helpers in [`android/gleam/browse/src/browse.gleam`](android/gleam/browse/src/browse.gleam) (`header`, `md_body`, `file_list`, `commit_list`, …). Those pack to int opcodes; Rust [`android/src/view_api.rs`](android/src/view_api.rs) decodes them onto Vidya widgets. Dynamic text/rows live in `ViewModel` (host), not in Wasm.

## Layout

| Path | Role |
|------|------|
| `android/` | Shared lib + Android NativeActivity (`cargo-apk`) |
| `host/` | Desktop binary (`browse`) |
| `android/gleam/browse/src/browse.gleam` | TEA + view DSL → opcodes |
| `android/src/components/` | Meta, Readme, RepoBrowser (Files/Commits/Patches/Issues/Jobs; status tabs + search) |
| `android/src/view_api.rs` | Host paint API (`Op` / `ViewModel` → components) |
| `android/src/markdown.rs` | README via pulldown-cmark → Vidya text |
| `android/src/rad.rs` | `Profile::load` + open by RID → snapshot |
| `android/src/gleam_guest.rs` | wasmtime + `browse__*` exports |
| `android/src/gleam_bridge.rs` | opcode fetch → `view_api` |
| `android/src/app.rs` | window, RID field, Open effect |
