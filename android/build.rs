//! Compile the Gleam Wasm guest (`gleam/browse`), or use the prebuilt fallback.
//!
//! Prefer a live `gleam build` when a wasm-capable Gleam is available; otherwise
//! copy `gleam/browse/prebuilt/browse.wasm` (kept for hermetic `nix build`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=GLEAM");
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("gleam/browse/src/browse.gleam").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("gleam/browse/gleam.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("gleam/browse/prebuilt/browse.wasm")
            .display()
    );

    let wasm_src = match try_gleam_build(&manifest_dir) {
        Some(p) => p,
        None => {
            let prebuilt = manifest_dir.join("gleam/browse/prebuilt/browse.wasm");
            if !prebuilt.is_file() {
                panic!(
                    "no Gleam Wasm guest: set GLEAM to a wasm-capable gleam, or add {}",
                    prebuilt.display()
                );
            }
            eprintln!(
                "browse build.rs: using prebuilt {}",
                prebuilt.display()
            );
            prebuilt
        }
    };

    let wasm_dst = out_dir.join("browse.wasm");
    fs::copy(&wasm_src, &wasm_dst).unwrap_or_else(|err| {
        panic!("copy {} → {}: {err}", wasm_src.display(), wasm_dst.display());
    });

    let inspect = manifest_dir.join("target/browse.wasm");
    if let Some(parent) = inspect.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(&wasm_src, &inspect);

    eprintln!(
        "browse build.rs: browse.wasm ready ({} bytes)",
        fs::metadata(&wasm_dst).map(|m| m.len()).unwrap_or(0)
    );
}

fn try_gleam_build(manifest_dir: &Path) -> Option<PathBuf> {
    let gleam = find_gleam(manifest_dir)?;
    let guest_dir = manifest_dir.join("gleam/browse");
    eprintln!("browse build.rs: gleam → {}", gleam.display());

    let status = Command::new(&gleam)
        .current_dir(&guest_dir)
        .arg("build")
        .status()
        .ok()?;
    if !status.success() {
        eprintln!("browse build.rs: `gleam build` failed; falling back to prebuilt");
        return None;
    }

    let wasm = guest_dir.join("build/dev/wasm/browse/browse.wasm");
    if wasm.is_file() {
        Some(wasm)
    } else {
        None
    }
}

fn find_gleam(manifest_dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = env::var("GLEAM") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    for cand in [
        manifest_dir.join("../gleam/target/debug/gleam"),
        manifest_dir.join("../gleam/target/release/gleam"),
        PathBuf::from("/home/nandi/code/gleam/target/debug/gleam"),
        PathBuf::from("/home/nandi/code/gleam/target/release/gleam"),
    ] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    which("gleam")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|dir| {
        let p = dir.join(name);
        p.is_file().then_some(p)
    })
}
