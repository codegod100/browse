# Browse

Minimal Radicle repo viewer: paste a `rad:z…` ID (or pass it on the CLI) and see name, description, files (README opened when present), and commits. If the repo is not in local storage yet, **Open** seeds/fetches it from the network and checks out a working copy under `~/code/<name>`.

**Stack:** [Vidya](https://tangled.org/nandi.uk/vidya) (egui shell) · Gleam TEA guest (screens / layout opcodes) · [`radicle`](https://app.radicle.xyz) crates (Profile + storage).

## Run

```bash
nix develop          # egui libs + rust + just; sets GLEAM if ~/code/gleam exists
just host
just host rad:z1ocacrfUUDHaSpspzjX5bUYf36w
just smoke rad:z1ocacrfUUDHaSpspzjX5bUYf36w

nix run . -- --smoke rad:z1ocacrfUUDHaSpspzjX5bUYf36w
nix build            # hermetic binary (uses gleam/browse/prebuilt/browse.wasm)
```

Needs a local Radicle profile (`~/.radicle`). Cloning from the network requires `rad node start`. Live Gleam rebuilds use a wasm-capable Gleam (`~/code/gleam` on branch `wasm`); `nix build` falls back to the vendored prebuilt Wasm.

## View API

Gleam builds screens with view helpers in [`gleam/browse/src/browse.gleam`](gleam/browse/src/browse.gleam) (`header`, `md_body`, `file_list`, `commit_list`, …). Those pack to int opcodes; Rust [`src/view_api.rs`](src/view_api.rs) decodes them onto Vidya widgets. Dynamic text/rows live in `ViewModel` (host), not in Wasm.

## Layout

| Path | Role |
|------|------|
| `gleam/browse/src/browse.gleam` | TEA + view DSL → opcodes |
| `src/components/` | Meta, Readme, RepoBrowser (Files/Commits tabs) |
| `src/view_api.rs` | Host paint API (`Op` / `ViewModel` → components) |
| `src/markdown.rs` | README via pulldown-cmark → Vidya text |
| `src/rad.rs` | `Profile::load` + open by RID → snapshot |
| `src/gleam_guest.rs` | wasmtime + `browse__*` exports |
| `src/gleam_bridge.rs` | opcode fetch → `view_api` |
| `src/app.rs` | window, RID field, Open effect |
