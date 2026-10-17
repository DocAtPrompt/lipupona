//! Converts every `.md` file of a folder and checks each result against the
//! laws of SPEC §9.2: canonical (L4), in the profile, and read back from its
//! display (L1). Prints what fails and a tally of the report codes.
//!
//! `cargo run -p md2lipupona --example corpus -- <folder> [commonmark|gfm|obsidian] [resolve]`
//!
//! With `resolve`, every wikilink target is found: `Ziel` as `ziel.html`, a
//! name with an extension as it is.

use std::collections::BTreeMap;
use std::process::ExitCode;

use md2lipupona::{Flavour, Settings, convert_with};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: corpus <folder> [commonmark|gfm|obsidian] [resolve]");
        return ExitCode::FAILURE;
    };
    let flavour = match args.get(1).map(String::as_str) {
        Some("gfm") => Flavour::Gfm,
        Some("obsidian") => Flavour::Obsidian,
        _ => Flavour::CommonMark,
    };
    let resolve = |target: &str| -> Option<String> {
        let page = target.split('#').next().unwrap_or(target);
        Some(match page.contains('.') {
            true => page.to_string(),
            false => format!("{}.html", page.to_lowercase()),
        })
    };
    let settings = Settings {
        flavour,
        resolve: (args.get(2).map(String::as_str) == Some("resolve"))
            .then_some(&resolve as &dyn Fn(&str) -> Option<String>),
    };
    let mut files: Vec<_> = match std::fs::read_dir(dir) {
        Ok(r) => r
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
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
        let Ok(md) = std::fs::read_to_string(path) else {
            continue;
        };
        let out = convert_with(&md, &settings);
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
