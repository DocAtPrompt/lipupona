//! Canonicalises an HTML file and prints the body and the diagnostics.
//!
//! `cargo run --example canon -- page.html`

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: canon <file.html>");
        return ExitCode::FAILURE;
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let out = lipupona::canon(&src);
    print!("{}", out.text);
    for d in &out.diagnostics {
        eprintln!("{d}");
    }
    ExitCode::SUCCESS
}
