//! Browse — Radicle repo viewer. Vidya paints; Gleam owns screens; radicle crates load RIDs.

mod app;
mod gleam_bridge;
mod gleam_guest;
mod rad;

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1).peekable();
    let smoke = args.peek().is_some_and(|a| a == "--smoke");
    if smoke {
        args.next();
    }

    let rid = args.next().filter(|a| a.starts_with("rad:") || a.starts_with('z'));
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
    println!("tree:  {} entries", view.tree.len());
    for e in view.tree.iter().take(12) {
        println!("  - {e}");
    }
    let preview: String = view.readme.chars().take(200).collect();
    println!("readme preview:\n{preview}");

    let model = gleam_guest::init().expect("gleam init");
    let model = gleam_guest::update(model, gleam_bridge::MSG_LOADED).expect("gleam loaded");
    let len = gleam_guest::view_len(model).expect("view_len");
    println!("gleam screen=viewing opcodes={len}");
    Ok(())
}
