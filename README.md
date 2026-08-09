# Browse

Minimal Radicle repo viewer: paste a `rad:z…` ID (or pass it on the CLI) and see name, description, README, and root tree for a repo already in local storage.

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

Needs a local Radicle profile with the repo seeded. Live Gleam rebuilds use a wasm-capable Gleam (`~/code/gleam` on branch `wasm`); `nix build` falls back to the vendored prebuilt Wasm.

## Layout

| Path | Role |
|------|------|
| `gleam/browse/` | Gleam model / update / view opcodes |
| `src/rad.rs` | `Profile::load` + open by RID → snapshot |
| `src/gleam_guest.rs` | wasmtime + `browse__*` exports |
| `src/gleam_bridge.rs` | opcodes → Vidya widgets (+ string slots) |
| `src/app.rs` | window, RID field, Open effect |
