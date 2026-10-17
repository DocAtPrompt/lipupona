//! Prints the display of an HTML file, or with `--html` the HTML of a
//! notation file, and the diagnostics.
//!
//! `cargo run --example display -- page.html`
//! `cargo run --example display -- --html page.txt`

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (to_html, path) = match args.as_slice() {
        [flag, path] if flag == "--html" => (true, path),
        [path] => (false, path),
        _ => {
            eprintln!("usage: display [--html] <file>");
            return ExitCode::FAILURE;
        }
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let out = if to_html {
        lipupona::html(&src)
    } else {
        lipupona::display(&src)
    };
    print!("{}", out.text);
    for d in &out.diagnostics {
        eprintln!("{d}");
    }
    ExitCode::SUCCESS
}
