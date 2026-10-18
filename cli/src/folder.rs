//! Folders: the files below a folder, and the resolver that finds a
//! wikilink's target among them as Obsidian does — by name in the whole
//! folder, or by the end of its path.

use std::io;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

/// The files below `root`, as paths relative to it, sorted. Hidden entries
/// (`.obsidian`, `.git`, `.DS_Store`) are left out, and so are links to
/// folders, which could lead in a circle: each goes into `skipped`.
pub fn walk(root: &Path, skipped: &mut Vec<PathBuf>) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_in(root, Path::new(""), &mut files, skipped)?;
    Ok(files)
}

fn walk_in(
    root: &Path,
    rel: &Path,
    files: &mut Vec<PathBuf>,
    skipped: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(root.join(rel))?.collect::<io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = rel.join(&name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk_in(root, &path, files, skipped)?;
        } else if kind.is_file() || std::fs::metadata(root.join(&path)).is_ok_and(|m| m.is_file()) {
            files.push(path);
        } else {
            skipped.push(path);
        }
    }
    Ok(())
}

/// Whether `path` has the extension `ext`, in any case.
pub fn has_extension(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// The name a converted file gets: a file with one of `exts` becomes
/// `.html`, any other keeps its name.
pub fn html_name(path: &Path, exts: &[&str]) -> PathBuf {
    match has_extension(path, exts) {
        true => path.with_extension("html"),
        false => path.to_path_buf(),
    }
}

/// The files of a folder, for finding wikilink targets.
pub struct Index {
    /// Each file's path with `/`, as written and as compared (NFC, lower
    /// case).
    files: Vec<(String, String)>,
}

impl Index {
    pub fn new(files: &[PathBuf]) -> Self {
        let files = files
            .iter()
            .map(|p| {
                let path = slashed(p);
                let key = key(&path);
                (path, key)
            })
            .collect();
        Index { files }
    }

    /// The `href` from the page converted from `from` to the target of a
    /// wikilink or embed as written (`Ferritin`, `Labor/Ferritin#Normwerte`,
    /// `hb.png`). A note is found by its name without `.md`, any other file
    /// by its name; where several match, the one with the shortest path.
    /// The section after `#` is dropped, since headings carry no ids.
    pub fn resolve(&self, from: &Path, target: &str) -> Option<String> {
        let page = target.split('#').next().unwrap_or(target).trim();
        let want = key(page.trim_start_matches("./").trim_start_matches('/'));
        if want.is_empty() {
            return None;
        }
        let found = self
            .find(&want)
            .or_else(|| self.find(&format!("{want}.md")))?;
        let to = slashed(&html_name(Path::new(found), &["md"]));
        Some(encode(&relative(&slashed(from), &to)))
    }

    fn find(&self, want: &str) -> Option<&str> {
        let tail = format!("/{want}");
        self.files
            .iter()
            .filter(|(_, k)| k == want || k.ends_with(&tail))
            .min_by_key(|(p, _)| (p.matches('/').count(), p.clone()))
            .map(|(p, _)| p.as_str())
    }
}

fn slashed(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn key(s: &str) -> String {
    s.replace('\\', "/")
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

/// The path from the folder of the file `from` to `to`, both relative to
/// the same root.
fn relative(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').collect();
    let dir = &from[..from.len().saturating_sub(1)];
    let to: Vec<&str> = to.split('/').collect();
    let common = dir
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a == b)
        .count()
        .min(to.len().saturating_sub(1));
    let mut parts = vec![".."; dir.len() - common];
    parts.extend(&to[common..]);
    parts.join("/")
}

/// Keeps the characters that would split a path (C12) part of the name.
fn encode(path: &str) -> String {
    path.replace('%', "%25")
        .replace('#', "%23")
        .replace('?', "%3F")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index {
        Index::new(&[
            PathBuf::from("Befunde/Anämie.md"),
            PathBuf::from("Labor/Ferritin.md"),
            PathBuf::from("Archiv/2025/Ferritin.md"),
            PathBuf::from("Dr. Müller.md"),
            PathBuf::from("bilder/hb 100%.png"),
            PathBuf::from("Start.md"),
        ])
    }

    #[test]
    fn finds_targets_as_obsidian_does() {
        let from = Path::new("Befunde/Anämie.md");
        let i = index();
        let r = |t: &str| i.resolve(from, t);
        assert_eq!(r("Ferritin").as_deref(), Some("../Labor/Ferritin.html"));
        assert_eq!(
            r("ferritin#Normwerte").as_deref(),
            Some("../Labor/Ferritin.html")
        );
        assert_eq!(
            r("2025/Ferritin").as_deref(),
            Some("../Archiv/2025/Ferritin.html")
        );
        assert_eq!(
            r("Labor/Ferritin.md").as_deref(),
            Some("../Labor/Ferritin.html")
        );
        assert_eq!(r("Dr. Müller").as_deref(), Some("../Dr. Müller.html"));
        assert_eq!(r("hb 100%.png").as_deref(), Some("../bilder/hb 100%25.png"));
        assert_eq!(r("Anämie").as_deref(), Some("Anämie.html"));
        // macOS may hand over a name decomposed.
        assert_eq!(r("Ana\u{308}mie").as_deref(), Some("Anämie.html"));
        assert_eq!(r("Fehlt"), None);
        assert_eq!(r("#Abschnitt"), None);
        assert_eq!(
            i.resolve(Path::new("Start.md"), "Ferritin").as_deref(),
            Some("Labor/Ferritin.html")
        );
    }
}
