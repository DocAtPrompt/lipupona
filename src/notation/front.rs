//! The front matter (SPEC §5.15): a document's head as notation writes it.

use super::block::SP;
use crate::diag::{Code, Diagnostic};
use crate::dom::Attr;
use crate::head::{Head, HeadItem, is_name};
use crate::html;

/// A front matter that was read: the head, the number of lines it takes
/// (both `---` lines included), what was reported, and the values as
/// slices of the lines, for an editor's spans.
pub(crate) struct Front<'a> {
    pub head: Head,
    pub used: usize,
    pub diags: Vec<Diagnostic>,
    pub values: Vec<&'a str>,
}

fn is_fence(line: &str) -> bool {
    line.trim_end_matches(SP) == "---"
}

/// Reads the front matter at the start of `lines`, if there is one: the
/// first line `---`, a later line `---`, and head lines between them.
pub(crate) fn read<'a>(lines: &[&'a str]) -> Option<Front<'a>> {
    if !is_fence(lines.first()?) {
        return None;
    }
    let close = (1..lines.len()).find(|&k| is_fence(lines[k]))?;
    let mut front = Front {
        head: Head::default(),
        used: close + 1,
        diags: Vec::new(),
        values: Vec::new(),
    };
    // The lines between the fences as one text, with the offset of each
    // line, so that head content may run over lines (a start tag too).
    let text = lines[..close].join("\n");
    let mut starts = Vec::with_capacity(close + 1);
    let mut at = 0;
    for line in &lines[..close] {
        starts.push(at);
        at += line.len() + 1;
    }
    starts.push(text.len() + 1);
    let within = Within {
        text: &text,
        starts: &starts,
    };
    // The key whose list items follow, and whether it took any.
    let mut list: Option<String> = None;
    let mut i = 1;
    while i < close {
        let line = lines[i];
        if line.trim_matches(SP).is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with('<') {
            list = None;
            i = foreign(lines, i, &within, &mut front.head)?;
            continue;
        }
        let rest = line.trim_start_matches(SP);
        if let Some(item) = rest.strip_prefix("- ").or((rest == "-").then_some("")) {
            let key = list.clone()?;
            let (value, shown) = scalar(item.trim_matches(SP));
            front.values.push(shown);
            add(&mut front, &key, value);
            i += 1;
            continue;
        }
        let (key, value) = entry(line)?;
        list = None;
        let value = value.trim_matches(SP);
        if value.is_empty() {
            list = Some(key);
        } else if let Some(items) = flow_list(value) {
            for (v, shown) in items {
                front.values.push(shown);
                add(&mut front, &key, v);
            }
        } else {
            let (v, shown) = scalar(value);
            front.values.push(shown);
            add(&mut front, &key, v);
        }
        i += 1;
    }
    Some(front)
}

/// `key: value` or `key:` at the end of the line: the key in lower case and
/// the rest.
fn entry(line: &str) -> Option<(String, &str)> {
    let (key, value) = match line.find(": ") {
        Some(k) => (&line[..k], &line[k + 2..]),
        None => (line.strip_suffix(':')?, ""),
    };
    is_name(key).then(|| (key.to_ascii_lowercase(), value))
}

fn add(front: &mut Front, key: &str, value: String) {
    let head = &mut front.head;
    match key {
        "title" | "lang" => {
            let taken = match key {
                "title" => head.items.iter().any(|i| matches!(i, HeadItem::Title(_))),
                _ => head.lang.is_some(),
            };
            if taken {
                front.diags.push(Diagnostic::new(
                    Code::ConstraintViolated,
                    format!("a second value of {key}: {value}"),
                ));
            } else if key == "title" {
                head.items.push(HeadItem::Title(value));
            } else {
                head.lang = Some(value);
            }
        }
        _ => head.items.push(HeadItem::Entry {
            name: key.to_string(),
            value,
        }),
    }
}

/// A value, plain or quoted, and the slice of it that is its text.
fn scalar(s: &str) -> (String, &str) {
    if let Some(inner) = s.strip_prefix('"')
        && let Some((text, len)) = double_quoted(inner)
        && inner[len..].trim_matches(SP).is_empty()
    {
        return (text, &inner[..len.saturating_sub(1)]);
    }
    if let Some(inner) = s.strip_prefix('\'')
        && let Some((text, len)) = single_quoted(inner)
        && inner[len..].trim_matches(SP).is_empty()
    {
        return (text, &inner[..len.saturating_sub(1)]);
    }
    (s.to_string(), s)
}

/// The text of `"…"` after its opening quote, and the bytes up to and with
/// its closing quote.
fn double_quoted(s: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((k, c)) = chars.next() {
        match c {
            '"' => return Some((out, k + 1)),
            '\\' => match chars.next() {
                Some((_, e @ ('"' | '\\'))) => out.push(e),
                Some((_, e)) => {
                    out.push('\\');
                    out.push(e);
                }
                None => out.push('\\'),
            },
            c => out.push(c),
        }
    }
    None
}

fn single_quoted(s: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut k = 0;
    let b = s.as_bytes();
    while k < s.len() {
        if b[k] == b'\'' {
            if b.get(k + 1) == Some(&b'\'') {
                out.push('\'');
                k += 2;
                continue;
            }
            return Some((out, k + 1));
        }
        let c = s[k..].chars().next()?;
        out.push(c);
        k += c.len_utf8();
    }
    None
}

/// `[a, "b, c"]`: its items, each with the slice that is its text.
fn flow_list(s: &str) -> Option<Vec<(String, &str)>> {
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let mut items = Vec::new();
    let mut rest = inner;
    loop {
        rest = rest.trim_start_matches(SP);
        if rest.is_empty() {
            break;
        }
        let end = if let Some(r) = rest.strip_prefix('"') {
            1 + double_quoted(r)?.1
        } else if let Some(r) = rest.strip_prefix('\'') {
            1 + single_quoted(r)?.1
        } else {
            rest.find(',').unwrap_or(rest.len())
        };
        let item = rest[..end].trim_matches(SP);
        if !item.is_empty() {
            items.push(scalar(item));
        }
        rest = &rest[end..];
        rest = rest.trim_start_matches(SP);
        match rest.strip_prefix(',') {
            Some(r) => rest = r,
            None if rest.is_empty() => break,
            None => return None,
        }
    }
    Some(items)
}

/// The lines of a front matter before its closing fence as one text, and
/// where each line begins in it (one more for the end).
struct Within<'t> {
    text: &'t str,
    starts: &'t [usize],
}

impl Within<'_> {
    /// The line that holds byte `at` of the text.
    fn line_of(&self, at: usize) -> usize {
        self.starts.partition_point(|&s| s <= at) - 1
    }
}

/// Head content at `lines[i]`, which begins with `<`: an `html`, `head` or
/// `body` start tag gives attributes; anything else — a stylesheet, or
/// content outside the profile — is taken as stored, over the lines up to
/// its end: the end of its start tag for a void element, else its end tag.
/// The index after it, or None when it does not end before the closing
/// `---`.
fn foreign(lines: &[&str], i: usize, within: &Within, head: &mut Head) -> Option<usize> {
    let from = within.starts[i];
    let rest = &within.text[from..];
    let line = lines[i];
    if line.starts_with("<!--") {
        let end = rest[4..].find("-->")? + 4 + 3;
        let j = within.line_of(from + end - 1);
        head.items.push(HeadItem::Foreign(lines[i..=j].join("\n")));
        return Some(j + 1);
    }
    let (name, attrs, len) = html::start_tag_at(rest)?;
    let tag_end = within.line_of(from + len - 1);
    let set: Option<&mut Vec<Attr>> = match name.as_str() {
        "html" => Some(&mut head.html_attrs),
        "head" => Some(&mut head.head_attrs),
        "body" => Some(&mut head.body_attrs),
        _ => None,
    };
    if let Some(set) = set {
        let after = &lines[tag_end][from + len - within.starts[tag_end]..];
        if !after.trim_matches(SP).is_empty() {
            return None;
        }
        for a in attrs {
            if name == "html" && a.name == "lang" {
                head.lang.get_or_insert(a.value);
            } else {
                set.push(a);
            }
        }
        return Some(tag_end + 1);
    }
    let j = match html::is_void(&name) {
        true => tag_end,
        false => {
            let end = html::end_tag_in(&rest[len..], &name)?;
            within.line_of(from + len + end)
        }
    };
    head.items.push(HeadItem::Foreign(lines[i..=j].join("\n")));
    Some(j + 1)
}

/// The front matter of `head` (§5.15), with its two `---` lines.
pub(crate) fn write(head: &Head) -> String {
    let mut out = String::from("---\n");
    if let Some(lang) = &head.lang {
        out.push_str(&format!("lang: {}\n", quote(lang, false)));
    }
    let mut k = 0;
    while k < head.items.len() {
        match &head.items[k] {
            HeadItem::Title(t) => out.push_str(&format!("title: {}\n", quote(t, false))),
            item @ (HeadItem::Foreign(_) | HeadItem::Style { .. } | HeadItem::Link { .. }) => {
                out.push_str(&item.html());
                out.push('\n');
            }
            HeadItem::Entry { name, .. } => {
                let run: Vec<&str> = head.items[k..]
                    .iter()
                    .map_while(|i| match i {
                        HeadItem::Entry { name: n, value } if n == name => Some(value.as_str()),
                        _ => None,
                    })
                    .collect();
                match run.as_slice() {
                    [one] => out.push_str(&format!("{name}: {}\n", quote(one, false))),
                    many => {
                        let items: Vec<String> = many.iter().map(|v| quote(v, true)).collect();
                        out.push_str(&format!("{name}: [{}]\n", items.join(", ")));
                    }
                }
                k += run.len();
                continue;
            }
        }
        k += 1;
    }
    for (name, attrs) in [
        ("html", &head.html_attrs),
        ("head", &head.head_attrs),
        ("body", &head.body_attrs),
    ] {
        if attrs.is_empty() {
            continue;
        }
        let mut el = crate::dom::Element {
            name: name.to_string(),
            attrs: attrs.clone(),
            children: Vec::new(),
            span: 0..0,
            implied_end: false,
        };
        crate::canon::normalize_attrs(&mut el);
        out.push_str(&crate::canon::start_tag(&el));
        out.push('\n');
    }
    out.push_str("---\n");
    out
}

/// A value as the front matter writes it: plain where it reads back so, else
/// in double quotes (§5.15).
fn quote(v: &str, in_list: bool) -> String {
    let edge = |c: Option<char>| c.is_some_and(|c| c == ' ' || c == '\t');
    let needs = v.is_empty()
        || edge(v.chars().next())
        || edge(v.chars().last())
        || v.starts_with([
            '"', '\'', '[', ']', '{', '}', '-', '#', '&', '*', '!', '|', '>', '%', '@', '`',
        ])
        || v.contains(": ")
        || v.contains(" #")
        || in_list && v.contains(',');
    if !needs {
        return v.to_string();
    }
    let mut out = String::from("\"");
    for c in v.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}
