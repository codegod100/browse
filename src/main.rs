//! Browse — Radicle repo viewer. Vidya paints; Gleam owns screens; radicle crates load RIDs.

mod app;
mod components;
mod gleam_bridge;
mod gleam_guest;
mod markdown;
mod rad;
mod view_api;

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1).peekable();
    let smoke = args.peek().is_some_and(|a| a == "--smoke");
    if smoke {
        args.next();
    }

    let rid = args
        .next()
        .filter(|a| a.starts_with("rad:") || a.starts_with('z'));
    let rid = rid.map(|a| {
        if a.starts_with("rad:") {
            a
        } else {
            format!("rad:{a}")
        }
    });

    if smoke {
        return run_smoke(rid.as_deref());
    }

    app::run(rid)
}

fn run_smoke(rid: Option<&str>) -> eframe::Result {
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
