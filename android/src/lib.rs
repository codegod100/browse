//! Browse — Radicle repo viewer (desktop host + Android NativeActivity).

mod app;
mod components;
mod gleam_bridge;
mod gleam_guest;
mod linkify;
mod markdown;
mod rad;
mod recent;
mod tab_prefs;
mod view_api;

pub use app::run_desktop;

#[cfg(target_os = "android")]
pub use app::run_android;

/// Headless smoke: open a RID from local storage and print a short dump.
pub fn run_smoke(rid: Option<&str>) -> eframe::Result {
    let profile = rad::load_profile().expect("load profile");
    let rid = rid.expect("usage: browse --smoke rad:z…");
    let view = rad::open_repo(&profile, rid).unwrap_or_else(|e| {
        eprintln!("open failed: {e}");
        std::process::exit(1);
    });
    println!("rid:   {}", view.rid);
    println!("name:  {}", view.name);
    println!("desc:  {}", view.description);
    println!("head:  {}", view.head);
    println!("files: {} entries", view.files.len());
    for f in view.files.iter().take(12) {
        let kind = if f.is_tree { "/" } else { "" };
        println!("  - {}{kind}", f.name);
    }
    println!("commits: {}", view.commits.len());
    for c in view.commits.iter().take(8) {
        println!("  {}  {} — {}", c.short_id, c.summary, c.author);
    }
    println!("patches: {}", view.patches.len());
    for p in view.patches.iter().take(8) {
        println!(
            "  [{}] {}  {} — {}",
            p.state, p.short_id, p.title, p.author
        );
    }
    println!("issues: {}", view.issues.len());
    for i in view.issues.iter().take(8) {
        println!(
            "  [{}] {}  {} — {}",
            i.state, i.short_id, i.title, i.author
        );
    }
    println!("jobs: {}", view.jobs.len());
    for j in view.jobs.iter().take(8) {
        println!(
            "  [{}] {}  commit {} — {} run(s)",
            j.status, j.short_id, j.short_commit, j.run_count
        );
    }
    if let Some(c) = view.commits.first() {
        match rad::commit_paths(&profile, &view.rid, &c.id) {
            Ok(paths) => {
                println!("commit {} paths: {}", c.short_id, paths.len());
                for p in paths.iter().take(6) {
                    println!("    {p}");
                }
                if let Some(p) = paths.first() {
                    match rad::file_patch(&profile, &view.rid, &c.id, p) {
                        Ok(diff) => {
                            let preview: String = diff.chars().take(160).collect();
                            println!("diff {p} preview:\n{preview}");
                        }
                        Err(e) => println!("diff error: {e}"),
                    }
                }
            }
            Err(e) => println!("paths error: {e}"),
        }
    }
    if let Some(f) = view.files.iter().find(|f| !f.is_tree) {
        match rad::read_file(&profile, &view.rid, &view.head_oid, &f.name) {
            Ok(text) => {
                let preview: String = text.chars().take(80).collect();
                println!("file {} preview: {preview}", f.name);
            }
            Err(e) => println!("file error: {e}"),
        }
    }
    let preview: String = view.readme.chars().take(200).collect();
    println!("readme preview:\n{preview}");

    let model = gleam_guest::init().expect("gleam init");
    let model = gleam_guest::update(model, gleam_bridge::MSG_LOADED).expect("gleam loaded");
    let len = gleam_guest::view_len(model).expect("view_len");
    println!("gleam screen=viewing opcodes={len}");
    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    // Point Radicle at app-private storage when RAD_HOME is unset.
    if std::env::var_os("RAD_HOME").is_none() {
        if let Some(dir) = android_app.internal_data_path() {
            // SAFETY: single-threaded before other threads touch the env.
            std::env::set_var("RAD_HOME", dir);
        }
    }
    log::info!("browse android_main start");
    match run_android(android_app) {
        Ok(()) => log::info!("browse run_android returned Ok"),
        Err(e) => log::error!("browse run_android error: {e}"),
    }
}
