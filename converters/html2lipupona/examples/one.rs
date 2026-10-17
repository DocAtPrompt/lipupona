//! Converts one HTML file and prints the HTML, the values for the head and
//! the report.
//!
//! `cargo run -p html2lipupona --example one -- page.html`

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: one <file.html>");
        return;
    };
    let html = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return;
        }
    };
    let out = html2lipupona::convert(&html, html2lipupona::Mode::Export);
    print!("{}", out.html);
    if let Some(t) = &out.meta.title {
        eprintln!("title: {t}");
    }
    if let Some(l) = &out.meta.lang {
        eprintln!("lang: {l}");
    }
    for e in &out.report {
        eprintln!("{}: {}", e.code, e.message);
    }
}
