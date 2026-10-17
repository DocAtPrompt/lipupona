//! Converts one Markdown file and prints the HTML and the report.
//!
//! `cargo run -p md2lipupona --example one -- page.md [commonmark|gfm]`

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: one <file.md> [commonmark|gfm]");
        return;
    };
    let flavour = match args.get(1).map(String::as_str) {
        Some("gfm") => md2lipupona::Flavour::Gfm,
        _ => md2lipupona::Flavour::CommonMark,
    };
    let md = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return;
        }
    };
    let out = md2lipupona::convert(&md, flavour);
    print!("{}", out.html);
    for e in &out.report {
        eprintln!("{}: {}", e.code, e.message);
    }
}
