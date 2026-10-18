//! The `lipupona` command: the core's three conversions, a check for stored
//! files, and the converters. Unlike the libraries it calls, it reads and
//! writes files (docs/ports.md §1).

mod folder;

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lipupona::head::{Head, HeadItem};
use md2lipupona::Flavour;

const USAGE: &str = "\
lipupona — a plain-text notation for profiled HTML

usage: lipupona <command> [options] [file]
       lipupona from-md|from-html [options] <folder> -o <folder>
       lipupona check [file|folder]...

commands:
  canon       canonical HTML of the body of an HTML file (SPEC §4)
  display     the notation of an HTML file (SPEC §5)
  html        the canonical HTML of a notation file (SPEC §5, §6)
  spans       the spans of a notation file for an editor (SPEC §5.14)
  from-md     Markdown to canonical HTML (docs/converters.md §2)
  from-html   HTML exported by an editor to canonical HTML (§3)
  check       report HTML files that are not canonical or leave the profile

options:
  -o, --output <file>   write to <file> instead of standard output
  --flavour <name>      from-md: commonmark (default), gfm or obsidian
  --definition-lists    from-md: read `Term` / `: definition` as a list
  --vault <folder>      from-md on a file: find wikilink targets among the
                        files of <folder>, which holds the file
  --article             from-html: a web page to keep; navigation, sidebars,
                        comments and the like are stripped and reported
  --document            from-md, from-html: write a whole document, its head
                        from the converter's values (title, lang, tags …)
  -h, --help            show this text
  -V, --version         show the version

Without a file, or with -, the input is standard input. A file that is a
whole document is checked with its head. Diagnostics and reports go to
standard error as `<file>: <code>: <message>`, check's findings to standard
output.

A folder is converted into a folder of the same shape: Markdown (from-md)
or HTML (from-html) files become .html files, other files are copied,
hidden ones are left out. Wikilinks and embeds find their target among the
folder's files by name, as in Obsidian; a single file finds them with
--vault, else they stay text.
check takes the .html files below a folder.

Exit status: 0 done, 1 check found something, 2 usage or file error.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Canon,
    Display,
    Html,
    Spans,
    FromMd,
    FromHtml,
    Check,
}

impl Command {
    fn name(self) -> &'static str {
        match self {
            Command::Canon => "canon",
            Command::Display => "display",
            Command::Html => "html",
            Command::Spans => "spans",
            Command::FromMd => "from-md",
            Command::FromHtml => "from-html",
            Command::Check => "check",
        }
    }

    /// The extensions of the files a conversion reads in a folder.
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Command::FromMd => &["md"],
            _ => &["html", "htm"],
        }
    }
}

#[derive(Debug)]
struct Args {
    command: Command,
    files: Vec<String>,
    output: Option<String>,
    flavour: Option<Flavour>,
    definition_lists: bool,
    article: bool,
    document: bool,
    vault: Option<String>,
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
        Some("spans") => Command::Spans,
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
        definition_lists: false,
        article: false,
        document: false,
        vault: None,
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
            "--vault" => parsed.vault = Some(value()?),
            "--flavour" => {
                parsed.flavour = Some(match value()?.as_str() {
                    "commonmark" => Flavour::CommonMark,
                    "gfm" => Flavour::Gfm,
                    "obsidian" => Flavour::Obsidian,
                    other => return Err(format!("unknown flavour `{other}`")),
                })
            }
            "--document" if inline.is_none() => parsed.document = true,
            "--definition-lists" if inline.is_none() => parsed.definition_lists = true,
            "--article" if inline.is_none() => parsed.article = true,
            _ => return Err(format!("unknown option `{arg}`")),
        }
    }
    let name = args[0].as_str();
    if parsed.flavour.is_some() && command != Command::FromMd {
        return Err(format!("`--flavour` is for from-md, not {name}"));
    }
    if parsed.definition_lists && command != Command::FromMd {
        return Err(format!("`--definition-lists` is for from-md, not {name}"));
    }
    if parsed.vault.is_some() && command != Command::FromMd {
        return Err(format!("`--vault` is for from-md, not {name}"));
    }
    if parsed.article && command != Command::FromHtml {
        return Err(format!("`--article` is for from-html, not {name}"));
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
    if path != "-" && Path::new(path).is_dir() {
        if !matches!(args.command, Command::FromMd | Command::FromHtml) {
            eprintln!(
                "lipupona: {} takes a file, not a folder",
                args.command.name()
            );
            return ExitCode::from(2);
        }
        if args.vault.is_some() {
            eprintln!("lipupona: `--vault` is for a single file; a folder finds its own links");
            return ExitCode::from(2);
        }
        return tree(args, Path::new(path));
    }
    let src = match read(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lipupona: {}: {e}", shown(path));
            return ExitCode::from(2);
        }
    };
    let vault = match args.vault.as_deref().map(|v| vault(v, path)).transpose() {
        Ok(found) => found,
        Err(message) => {
            eprintln!("lipupona: {message}");
            return ExitCode::from(2);
        }
    };
    let resolve = |target: &str| {
        let (index, rel) = vault.as_ref()?;
        index.resolve(rel, target)
    };
    let resolver: md2lipupona::Resolver = &resolve;
    let (text, notes) = convert(args, path, &src, vault.is_some().then_some(resolver));
    let target = args.output.as_deref();
    let written = write_out(target, &text);
    for note in notes {
        eprintln!("{}: {note}", shown(path));
    }
    finish(written, target.unwrap_or("-"))
}

/// The files of the folder `vault` and the place of `path` among them.
fn vault(vault: &str, path: &str) -> Result<(folder::Index, PathBuf), String> {
    if path == "-" {
        return Err("`--vault` needs a file that lies in it, not standard input".into());
    }
    let root = Path::new(vault)
        .canonicalize()
        .ok()
        .filter(|r| r.is_dir())
        .ok_or(format!("{vault}: not a folder"))?;
    let file = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    let rel = file
        .strip_prefix(&root)
        .map_err(|_| format!("{path} does not lie in the vault {vault}"))?
        .to_path_buf();
    let files = folder::walk(&root, &mut Vec::new()).map_err(|e| format!("{vault}: {e}"))?;
    Ok((folder::Index::new(&files), rel))
}

/// Converts every file below `input` that the command reads into the same
/// place below the output folder, as `.html`; other files are copied.
fn tree(args: &Args, input: &Path) -> ExitCode {
    let Some(output) = args.output.as_deref().filter(|o| *o != "-") else {
        eprintln!("lipupona: a folder is converted into a folder: -o <folder>");
        return ExitCode::from(2);
    };
    let output = Path::new(output);
    if output.exists() && !output.is_dir() {
        eprintln!("lipupona: {}: not a folder", output.display());
        return ExitCode::from(2);
    }
    // An output inside the input would be read as input on the next run.
    if let (Ok(i), Some(o)) = (input.canonicalize(), absolute(output))
        && o.starts_with(&i)
    {
        eprintln!("lipupona: the output folder lies inside the input folder");
        return ExitCode::from(2);
    }
    let mut skipped = Vec::new();
    let files = match folder::walk(input, &mut skipped) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("lipupona: {}: {e}", input.display());
            return ExitCode::from(2);
        }
    };
    for path in &skipped {
        eprintln!(
            "{}: skipped, a link to a folder",
            input.join(path).display()
        );
    }
    let exts = args.command.extensions();
    let index = folder::Index::new(&files);
    // Converted files first: they win over a copied file of the same name.
    let (read, copied): (Vec<_>, Vec<_>) =
        files.iter().partition(|p| folder::has_extension(p, exts));
    let mut written = HashSet::new();
    let mut failed = false;
    for rel in read.into_iter().chain(copied) {
        let from = input.join(rel);
        let name = folder::html_name(rel, exts);
        let to = output.join(&name);
        if !written.insert(name) {
            eprintln!(
                "{}: skipped, {} is written already",
                from.display(),
                to.display()
            );
            continue;
        }
        let result = match folder::has_extension(rel, exts) {
            true => std::fs::read_to_string(&from).map(|src| {
                let resolve = |target: &str| index.resolve(rel, target);
                convert(args, &from.to_string_lossy(), &src, Some(&resolve))
            }),
            false => Ok((String::new(), Vec::new())),
        };
        let done = result.and_then(|(text, notes)| {
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            match folder::has_extension(rel, exts) {
                true => std::fs::write(&to, text)?,
                false => _ = std::fs::copy(&from, &to)?,
            }
            Ok(notes)
        });
        match done {
            Ok(notes) => {
                for note in notes {
                    eprintln!("{}: {note}", from.display());
                }
            }
            Err(e) => {
                eprintln!("lipupona: {}: {e}", from.display());
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// `path` made absolute through its nearest existing ancestor.
fn absolute(path: &Path) -> Option<PathBuf> {
    let path = std::env::current_dir().ok()?.join(path);
    let mut rest = Vec::new();
    let mut base = path.as_path();
    loop {
        if let Ok(found) = base.canonicalize() {
            return Some(rest.iter().rev().fold(found, |p, c| p.join(c)));
        }
        rest.push(base.file_name()?);
        base = base.parent()?;
    }
}

/// The output text and the diagnostics or report entries, as `code: message`.
fn convert(
    args: &Args,
    path: &str,
    src: &str,
    resolve: Option<md2lipupona::Resolver>,
) -> (String, Vec<String>) {
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
    let document = |mut head: Head, body: &str| {
        if !head.items.iter().any(|i| matches!(i, HeadItem::Title(_))) {
            head.items.splice(0..0, stem().map(HeadItem::Title));
        }
        lipupona::document(&head, body).text
    };
    match args.command {
        Command::Canon => core(lipupona::canon(src)),
        Command::Display => core(lipupona::display(src)),
        Command::Html => core(lipupona::html(src)),
        Command::Spans => {
            let mut out = String::new();
            for span in lipupona::spans(src) {
                let kind = format!("{:?}", span.kind).to_lowercase();
                let text = src.get(span.range.clone()).unwrap_or("");
                out.push_str(&format!(
                    "{kind:<9} {:>5}..{:<5} {text:?}\n",
                    span.range.start, span.range.end
                ));
            }
            (out, Vec::new())
        }
        Command::FromMd => {
            let settings = md2lipupona::Settings {
                flavour: args.flavour.unwrap_or_default(),
                definition_lists: args.definition_lists,
                resolve,
            };
            let out = md2lipupona::convert_with(src, &settings);
            let notes = report(&out.report);
            if !args.document {
                return (out.html, notes);
            }
            let (head, more) = out.meta.head();
            (document(head, &out.html), [notes, report(&more)].concat())
        }
        Command::FromHtml => {
            let mode = match args.article {
                true => html2lipupona::Mode::Article,
                false => html2lipupona::Mode::Export,
            };
            let out = html2lipupona::convert(src, mode);
            let notes = report(&out.report);
            if !args.document {
                return (out.html, notes);
            }
            let (head, more) = out.meta.head();
            (document(head, &out.html), [notes, report(&more)].concat())
        }
        Command::Check => unreachable!("check has no single output"),
    }
}

/// Prints what keeps each file from being stored as it is: not canonical,
/// or blocks outside the profile. Exit status 1 if anything was found.
fn check(files: &[String]) -> ExitCode {
    let (mut found, mut failed) = (false, false);
    let mut paths = Vec::new();
    for path in files {
        if path == "-" || !Path::new(path).is_dir() {
            paths.push(path.clone());
            continue;
        }
        match folder::walk(Path::new(path), &mut Vec::new()) {
            Ok(below) => paths.extend(
                below
                    .iter()
                    .filter(|p| folder::has_extension(p, &["html", "htm"]))
                    .map(|p| Path::new(path).join(p).to_string_lossy().into_owned()),
            ),
            Err(e) => {
                eprintln!("lipupona: {path}: {e}");
                failed = true;
            }
        }
    }
    if files.is_empty() {
        paths.push("-".to_string());
    }
    let mut lines = String::new();
    for path in &paths {
        let src = match read(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lipupona: {}: {e}", shown(path));
                failed = true;
                continue;
            }
        };
        let out = lipupona::canon(&src);
        let mut findings = Vec::new();
        if out.text != src {
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
