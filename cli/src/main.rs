//! The `lipupona` command: the core's three conversions, a check for stored
//! files, and the converters. Unlike the libraries it calls, it reads and
//! writes files (docs/ports.md §1).

mod document;

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use md2lipupona::Flavour;

const USAGE: &str = "\
lipupona — a plain-text notation for profiled HTML

usage: lipupona <command> [options] [file]

commands:
  canon       canonical HTML of the body of an HTML file (SPEC §4)
  display     the notation of an HTML file (SPEC §5)
  html        the canonical HTML of a notation file (SPEC §5, §6)
  from-md     Markdown to canonical HTML (docs/converters.md §2)
  from-html   HTML exported by an editor to canonical HTML (§3)
  check       report HTML files that are not canonical or leave the profile

options:
  -o, --output <file>   write to <file> instead of standard output
  --flavour <name>      from-md: commonmark (default), gfm or obsidian
  --document            from-md, from-html: write a whole document, its head
                        from the converter's values (title, lang, tags …)
  -h, --help            show this text
  -V, --version         show the version

Without a file, or with -, the input is standard input; check takes several
files. A file that is a whole document is checked by its body. Diagnostics
and reports go to standard error as `<file>: <code>: <message>`, check's
findings to standard output. Wikilinks stay text: from-md has no resolver yet.

Exit status: 0 done, 1 check found something, 2 usage or file error.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Canon,
    Display,
    Html,
    FromMd,
    FromHtml,
    Check,
}

#[derive(Debug)]
struct Args {
    command: Command,
    files: Vec<String>,
    output: Option<String>,
    flavour: Option<Flavour>,
    document: bool,
}

#[derive(Debug)]
enum Parsed {
    Run(Args),
    Help,
    Version,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Parsed::Run(args)) => run(&args),
        Ok(Parsed::Help) => finish(write_out(None, USAGE), "-"),
        Ok(Parsed::Version) => finish(
            write_out(None, &format!("lipupona {}\n", env!("CARGO_PKG_VERSION"))),
            "-",
        ),
        Err(message) => {
            eprintln!("lipupona: {message}\ntry `lipupona --help`");
            ExitCode::from(2)
        }
    }
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut it = args.iter();
    let command = match it.next().map(String::as_str) {
        None => return Err("no command".into()),
        Some("-h" | "--help" | "help") => return Ok(Parsed::Help),
        Some("-V" | "--version") => return Ok(Parsed::Version),
        Some("canon") => Command::Canon,
        Some("display") => Command::Display,
        Some("html") => Command::Html,
        Some("from-md") => Command::FromMd,
        Some("from-html") => Command::FromHtml,
        Some("check") => Command::Check,
        Some(other) => return Err(format!("unknown command `{other}`")),
    };
    let mut parsed = Args {
        command,
        files: Vec::new(),
        output: None,
        flavour: None,
        document: false,
    };
    let mut only_files = false;
    while let Some(arg) = it.next() {
        if only_files || arg == "-" || !arg.starts_with('-') {
            parsed.files.push(arg.clone());
            continue;
        }
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if arg.starts_with("--") => (flag, Some(value)),
            _ => (arg.as_str(), None),
        };
        let mut value = || match inline {
            Some(v) => Ok(v.to_string()),
            None => it.next().cloned().ok_or(format!("`{flag}` needs a value")),
        };
        match flag {
            "--" if inline.is_none() => only_files = true,
            "-h" | "--help" if inline.is_none() => return Ok(Parsed::Help),
            "-o" | "--output" => parsed.output = Some(value()?),
            "--flavour" => {
                parsed.flavour = Some(match value()?.as_str() {
                    "commonmark" => Flavour::CommonMark,
                    "gfm" => Flavour::Gfm,
                    "obsidian" => Flavour::Obsidian,
                    other => return Err(format!("unknown flavour `{other}`")),
                })
            }
            "--document" if inline.is_none() => parsed.document = true,
            _ => return Err(format!("unknown option `{arg}`")),
        }
    }
    let name = args[0].as_str();
    if parsed.flavour.is_some() && command != Command::FromMd {
        return Err(format!("`--flavour` is for from-md, not {name}"));
    }
    if parsed.document && !matches!(command, Command::FromMd | Command::FromHtml) {
        return Err(format!(
            "`--document` is for from-md and from-html, not {name}"
        ));
    }
    if command == Command::Check {
        if parsed.output.is_some() {
            return Err("check writes no output file".into());
        }
    } else if parsed.files.len() > 1 {
        return Err(format!("{name} takes one file"));
    }
    Ok(Parsed::Run(parsed))
}

fn run(args: &Args) -> ExitCode {
    if args.command == Command::Check {
        return check(&args.files);
    }
    let path = args.files.first().map(String::as_str).unwrap_or("-");
    let src = match read(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lipupona: {}: {e}", shown(path));
            return ExitCode::from(2);
        }
    };
    let (text, notes) = convert(args, path, &src);
    let target = args.output.as_deref();
    let written = write_out(target, &text);
    for note in notes {
        eprintln!("{}: {note}", shown(path));
    }
    finish(written, target.unwrap_or("-"))
}

/// The output text and the diagnostics or report entries, as `code: message`.
fn convert(args: &Args, path: &str, src: &str) -> (String, Vec<String>) {
    let core = |out: lipupona::Output| {
        let notes = out.diagnostics.iter().map(|d| d.to_string()).collect();
        (out.text, notes)
    };
    let report = |entries: &[md2lipupona::Entry]| -> Vec<String> {
        entries
            .iter()
            .map(|e| format!("{}: {}", e.code, e.message))
            .collect()
    };
    // A file's name is the title of a page that has none, as in Obsidian.
    let stem = || {
        (path != "-")
            .then(|| Path::new(path).file_stem())
            .flatten()
            .map(|s| s.to_string_lossy().into_owned())
    };
    match args.command {
        Command::Canon => core(lipupona::canon(src)),
        Command::Display => core(lipupona::display(src)),
        Command::Html => core(lipupona::html(src)),
        Command::FromMd => {
            let out = md2lipupona::convert(src, args.flavour.unwrap_or_default());
            let notes = report(&out.report);
            if !args.document {
                return (out.html, notes);
            }
            let m = out.meta;
            let mut meta = Vec::new();
            let mut add = |name: &str, value: Option<String>| {
                if let Some(v) = value.filter(|v| !v.is_empty()) {
                    meta.push((name.to_string(), v));
                }
            };
            add("dcterms.created", m.created);
            add("dcterms.source", m.source);
            add("keywords", Some(m.tags.join(", ")));
            add("aliases", Some(m.aliases.join(", ")));
            for (key, value) in m.other {
                add(&key, Some(value));
            }
            let head = document::Head {
                title: m.title.or_else(stem),
                lang: m.lang,
                meta,
            };
            (document::write(&head, &out.html), notes)
        }
        Command::FromHtml => {
            let out = html2lipupona::convert(src, html2lipupona::Mode::Export);
            let notes = report(&out.report);
            if !args.document {
                return (out.html, notes);
            }
            let head = document::Head {
                title: out.meta.title.or_else(stem),
                lang: out.meta.lang,
                meta: Vec::new(),
            };
            (document::write(&head, &out.html), notes)
        }
        Command::Check => unreachable!("check has no single output"),
    }
}

/// Prints what keeps each file from being stored as it is: not canonical,
/// or blocks outside the profile. Exit status 1 if anything was found.
fn check(files: &[String]) -> ExitCode {
    let stdin = ["-".to_string()];
    let files = if files.is_empty() { &stdin[..] } else { files };
    let (mut found, mut failed) = (false, false);
    let mut lines = String::new();
    for path in files {
        let src = match read(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lipupona: {}: {e}", shown(path));
                failed = true;
                continue;
            }
        };
        let body = match document::body(&src) {
            Some(range) => &src[range],
            None => &src[..],
        };
        let out = lipupona::canon(body);
        let mut findings = Vec::new();
        if out.text != body {
            findings.push("not canonical".to_string());
        }
        findings.extend(
            out.diagnostics
                .iter()
                .filter(|d| d.code.is_blocking())
                .map(|d| d.to_string()),
        );
        for finding in &findings {
            lines.push_str(&format!("{}: {finding}\n", shown(path)));
        }
        found |= !findings.is_empty();
    }
    match write_out(None, &lines) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => finish(Err(e), "-"),
        _ if failed => ExitCode::from(2),
        _ if found => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    }
}

fn read(path: &str) -> io::Result<String> {
    if path == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        std::fs::read_to_string(path)
    }
}

/// Writes to a file, or to standard output for `None` and `-`.
fn write_out(target: Option<&str>, text: &str) -> io::Result<()> {
    match target {
        Some(path) if path != "-" => std::fs::write(path, text),
        _ => {
            let mut out = io::stdout().lock();
            out.write_all(text.as_bytes())?;
            out.flush()
        }
    }
}

/// Exit status for a write: a reader that stopped early is no error.
fn finish(written: io::Result<()>, target: &str) -> ExitCode {
    match written {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => {
            let target = if target == "-" { "<stdout>" } else { target };
            eprintln!("lipupona: {target}: {e}");
            ExitCode::from(2)
        }
        _ => ExitCode::SUCCESS,
    }
}

fn shown(path: &str) -> &str {
    if path == "-" { "<stdin>" } else { path }
}
