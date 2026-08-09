//! Desktop host: `just host` or `cargo run --manifest-path host/Cargo.toml`

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
        return browse::run_smoke(rid.as_deref());
    }

    browse::run_desktop(rid)
}
