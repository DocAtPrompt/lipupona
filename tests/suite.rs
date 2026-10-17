//! Runs the language-neutral test suite under `tests/<area>/<case>/`
//! (format: `tests/README.md`).
//!
//! `cargo test --test suite` runs every case. `LIPUPONA_CASES=tables/` runs
//! the cases whose path begins with the given prefix. Kinds whose
//! conversions are not implemented yet are counted as pending, not failed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lipupona::{Output, Profile};

/// The test suite's name for implementation-specific attributes.
const IMPLEMENTATION: &str = "test";

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let filter = std::env::var("LIPUPONA_CASES").unwrap_or_default();
    let profile = Profile::reference(IMPLEMENTATION);

    let mut cases = Vec::new();
    for area in dirs(&root) {
        for case in dirs(&area) {
            if case.join("case.toml").is_file() {
                cases.push(case);
            }
        }
    }

    let mut tally: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    let mut failures = Vec::new();
    for dir in &cases {
        let name = dir
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !name.starts_with(&filter) {
            continue;
        }
        let area = name.split('/').next().unwrap().to_string();
        let result = Case::load(dir).and_then(|c| c.run(&profile));
        let slot = match &result {
            Ok(Verdict::Pass) => 0,
            Ok(Verdict::Pending) => 2,
            Err(_) => 1,
        };
        tally.entry(area).or_default()[slot] += 1;
        if let Err(why) = result {
            failures.push((name, why));
        }
    }

    for (name, why) in &failures {
        println!("FAIL {name}\n{}\n", indent(why));
    }
    println!(
        "{:<14} {:>5} {:>5} {:>7}",
        "area", "pass", "fail", "pending"
    );
    let mut total = [0; 3];
    for (area, t) in &tally {
        println!("{area:<14} {:>5} {:>5} {:>7}", t[0], t[1], t[2]);
        for i in 0..3 {
            total[i] += t[i];
        }
    }
    println!(
        "{:<14} {:>5} {:>5} {:>7}",
        "total", total[0], total[1], total[2]
    );
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

enum Verdict {
    Pass,
    Pending,
}

struct Case {
    kind: String,
    raw: bool,
    dir: PathBuf,
}

impl Case {
    fn load(dir: &Path) -> Result<Case, String> {
        let toml = read(&dir.join("case.toml"))?;
        let mut kind = None;
        let mut raw = false;
        for line in toml.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "kind" => kind = Some(value.trim_matches('"').to_string()),
                "raw" => raw = value == "true",
                _ => {}
            }
        }
        let kind = kind.ok_or("case.toml has no kind")?;
        Ok(Case {
            kind,
            raw,
            dir: dir.to_path_buf(),
        })
    }

    /// A file's content under test: without its final line feed, unless the
    /// case is raw and the file is an input.
    fn file(&self, name: &str) -> Result<String, String> {
        let text = read(&self.dir.join(name))?;
        if self.raw && name.starts_with("input.") {
            return Ok(text);
        }
        text.strip_suffix('\n')
            .map(str::to_string)
            .ok_or_else(|| format!("{name} does not end with a line feed"))
    }

    fn diagnostics(&self) -> Result<Vec<String>, String> {
        let path = self.dir.join("diagnostics.txt");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut codes: Vec<String> = self
            .file("diagnostics.txt")?
            .lines()
            .map(str::to_string)
            .collect();
        codes.sort();
        Ok(codes)
    }

    fn run(&self, profile: &Profile) -> Result<Verdict, String> {
        match self.kind.as_str() {
            "canonical" => {
                let input = self.file("input.html")?;
                let expected = self.file("expected.html")?;
                let out = canon(&input, profile);
                same("canon(input.html)", &expected, &out.text)?;
                same(
                    "canon(expected.html)",
                    &expected,
                    &canon(&expected, profile).text,
                )?;
                self.same_diagnostics(&out)?;
                Ok(Verdict::Pass)
            }
            "passthrough" => {
                let input = self.file("input.html")?;
                let out = canon(&input, profile);
                same("canon(input.html)", &input, &out.text)?;
                self.same_diagnostics(&out)?;
                Ok(Verdict::Pass)
            }
            "diagnostic" if self.dir.join("input.html").exists() => {
                let out = canon(&self.file("input.html")?, profile);
                self.same_diagnostics(&out)?;
                Ok(Verdict::Pass)
            }
            "roundtrip" | "tolerance" | "diagnostic" => Ok(Verdict::Pending),
            other => Err(format!("unknown kind {other}")),
        }
    }

    fn same_diagnostics(&self, out: &Output) -> Result<(), String> {
        let expected = self.diagnostics()?;
        let mut actual: Vec<String> = out.diagnostics.iter().map(|d| d.code.to_string()).collect();
        actual.sort();
        if expected == actual {
            return Ok(());
        }
        let details: Vec<String> = out.diagnostics.iter().map(|d| d.to_string()).collect();
        Err(format!(
            "diagnostics differ\n  expected: {expected:?}\n  actual:   {actual:?}\n  {}",
            details.join("\n  ")
        ))
    }
}

/// `canon` as the test suite sees it: the body fragment without the final
/// line feed of C2.
fn canon(input: &str, profile: &Profile) -> Output {
    let mut out = lipupona::canon::canon(input, profile);
    if out.text.ends_with('\n') {
        out.text.pop();
    }
    out
}

fn same(what: &str, expected: &str, actual: &str) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    let e: Vec<&str> = expected.split('\n').collect();
    let a: Vec<&str> = actual.split('\n').collect();
    let line = e
        .iter()
        .zip(&a)
        .position(|(x, y)| x != y)
        .unwrap_or(e.len().min(a.len()));
    Err(format!(
        "{what} differs at line {}\n  expected: {:?}\n  actual:   {:?}\n--- expected\n{expected}\n--- actual\n{actual}",
        line + 1,
        e.get(line).unwrap_or(&"<end>"),
        a.get(line).unwrap_or(&"<end>"),
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

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
