//! Converts every `.html` file of a folder and checks each result against
//! the laws of SPEC §9.2: canonical (L4), in the profile, and read back from
//! its display (L1). Prints what fails and a tally of the report codes.
//!
//! `cargo run -p html2lipupona --example html-corpus -- <folder>`

use std::collections::BTreeMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: html-corpus <folder>");
        return ExitCode::FAILURE;
    };
    let mut files: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(r) => r
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "html"))
            .collect(),
        Err(e) => {
            eprintln!("{dir}: {e}");
            return ExitCode::FAILURE;
        }
    };
    files.sort();
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut failed = 0;
    for path in &files {
        let Ok(html) = std::fs::read_to_string(path) else {
            continue;
        };
        let out = html2lipupona::convert(&html, html2lipupona::Mode::Export);
        for e in &out.report {
            *codes.entry(e.code.clone()).or_default() += 1;
        }
        let canon = lipupona::canon(&out.html);
        let back = lipupona::html(&lipupona::display(&out.html).text);
        let mut why = Vec::new();
        if canon.text != out.html {
            why.push("not canonical (L4)".to_string());
        }
        if let Some(d) = canon.diagnostics.iter().find(|d| d.code.is_blocking()) {
            why.push(format!("outside the profile: {d}"));
        }
        if back.text != out.html {
            why.push("does not read back from its display (L1)".to_string());
        }
        if !why.is_empty() {
            failed += 1;
            println!("FAIL {}: {}", path.display(), why.join("; "));
        }
    }
    println!("{} files, {failed} failing", files.len());
    for (code, n) in codes {
        println!("  {code:<24} {n}");
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
