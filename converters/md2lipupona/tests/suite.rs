//! Runs the conversion cases under `tests/converters/md2lipupona/<case>/`
//! (format: `tests/README.md`, kind `conversion`).
//!
//! `cargo test -p md2lipupona --test suite` runs every case;
//! `LIPUPONA_CASES=footnote` runs the cases whose name begins with it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use md2lipupona::{Flavour, Meta, Settings, convert_with};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/converters/md2lipupona");
    let filter = std::env::var("LIPUPONA_CASES").unwrap_or_default();
    let mut passed = 0;
    let mut failures = Vec::new();
    for dir in dirs(&root) {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !name.starts_with(&filter) || !dir.join("case.toml").is_file() {
            continue;
        }
        match run(&dir) {
            Ok(()) => passed += 1,
            Err(why) => failures.push((name, why)),
        }
    }
    for (name, why) in &failures {
        println!("FAIL converters/md2lipupona/{name}\n{}\n", indent(why));
    }
    println!("md2lipupona  pass {passed:>4}  fail {:>4}", failures.len());
    if failures.is_empty() && passed > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(dir: &Path) -> Result<(), String> {
    let toml = file(dir, "case.toml")?;
    let mut kind = None;
    let mut flavour = Flavour::CommonMark;
    let mut pages = false;
    let mut definition_lists = false;
    for line in toml.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "kind" => kind = Some(value.to_string()),
            "flavour" => {
                flavour = match value {
                    "commonmark" => Flavour::CommonMark,
                    "gfm" => Flavour::Gfm,
                    "obsidian" => Flavour::Obsidian,
                    other => return Err(format!("unknown flavour {other}")),
                }
            }
            "resolve" if value == "pages" => pages = true,
            "resolve" => return Err(format!("unknown resolver {value}")),
            "definition_lists" => definition_lists = value == "true",
            _ => {}
        }
    }
    if kind.as_deref() != Some("conversion") {
        return Err(format!("kind {kind:?}, expected \"conversion\""));
    }
    let input = file(dir, "input.md")?;
    let expected = file(dir, "expected.html")?;
    let resolve = |target: &str| resolve_pages(target);
    let settings = Settings {
        flavour,
        resolve: pages.then_some(&resolve as &dyn Fn(&str) -> Option<String>),
        definition_lists,
    };
    let out = convert_with(&input, &settings);
    same(
        "convert(input.md)",
        &expected,
        out.html.trim_end_matches('\n'),
    )?;

    let mut want: Vec<String> = match dir.join("report.txt").exists() {
        true => file(dir, "report.txt")?
            .lines()
            .map(str::to_string)
            .collect(),
        false => Vec::new(),
    };
    want.sort();
    // The report of the conversion and that of the head it makes.
    let (head, head_report) = out.meta.head();
    let report: Vec<_> = out.report.iter().chain(&head_report).collect();
    let mut got: Vec<String> = report.iter().map(|e| e.code.clone()).collect();
    got.sort();
    if want != got {
        let details: Vec<String> = report
            .iter()
            .map(|e| format!("{}: {}", e.code, e.message))
            .collect();
        return Err(format!(
            "report differs\n  expected: {want:?}\n  actual:   {got:?}\n  {}",
            details.join("\n  ")
        ));
    }

    let want_meta = match dir.join("meta.txt").exists() {
        true => file(dir, "meta.txt")?,
        false => String::new(),
    };
    same("meta", &want_meta, &meta_lines(&out.meta))?;
    if dir.join("document.html").exists() {
        same(
            "document(head, body)",
            &file(dir, "document.html")?,
            lipupona::document(&head, &out.html)
                .text
                .trim_end_matches('\n'),
        )?;
    }

    // expected.html is canonical, in the profile, and keeps L1.
    let canon = lipupona::canon(&expected);
    same(
        "canon(expected.html)",
        &expected,
        canon.text.trim_end_matches('\n'),
    )?;
    if let Some(d) = canon.diagnostics.iter().find(|d| d.code.is_blocking()) {
        return Err(format!("expected.html is outside the profile: {d}"));
    }
    let display = lipupona::display(&expected).text;
    let back = lipupona::html(&display).text;
    same(
        "html(display(expected.html))",
        &expected,
        back.trim_end_matches('\n'),
    )
}

/// The resolver `pages` of `tests/README.md`.
fn resolve_pages(target: &str) -> Option<String> {
    if target.contains('?') {
        return None;
    }
    let (page, anchor) = match target.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (target, None),
    };
    let mut href = match page.contains('.') {
        true => page.to_string(),
        false => format!("{}.html", page.to_lowercase()),
    };
    if let Some(a) = anchor {
        href.push('#');
        href.push_str(a);
    }
    Some(href)
}

/// The values for the head as `meta.txt` writes them.
fn meta_lines(meta: &Meta) -> String {
    let mut lines = Vec::new();
    for (key, value) in [
        ("title", &meta.title),
        ("lang", &meta.lang),
        ("created", &meta.created),
        ("source", &meta.source),
    ] {
        if let Some(v) = value {
            lines.push(format!("{key}: {v}"));
        }
    }
    for (key, list) in [("aliases", &meta.aliases), ("tags", &meta.tags)] {
        if !list.is_empty() {
            lines.push(format!("{key}: {}", list.join(", ")));
        }
    }
    for (key, value) in &meta.other {
        lines.push(format!("{key}: {value}"));
    }
    lines.join("\n")
}

/// A file's content without its final line feed.
fn file(dir: &Path, name: &str) -> Result<String, String> {
    let text = fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
    text.strip_suffix('\n')
        .map(str::to_string)
        .ok_or_else(|| format!("{name} does not end with a line feed"))
}

fn same(what: &str, expected: &str, actual: &str) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "{what} differs\n--- expected\n{expected}\n--- actual\n{actual}"
    ))
}

fn dirs(path: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(path)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
