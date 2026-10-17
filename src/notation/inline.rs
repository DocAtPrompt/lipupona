//! Reading phrasing (SPEC §5.5, §5.12, §6): markers, code spans, links, the
//! inline long form, typed HTML, comments and backslash escapes. A line feed
//! in the text is a `<br>`.
//!
//! The same functions answer the writer's question of W2 — "does the reader
//! see a construct here?" — so that reader and writer cannot drift apart.

use super::scan::Scan;
use super::syntax::{self, Selector, Shape};
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Comment, Element, Node, Text};
use crate::profile::{Kind, Profile};
use crate::{html, url};

pub(crate) struct Inline<'p> {
    pub profile: &'p Profile,
    pub diags: Vec<Diagnostic>,
    /// The text being read, its byte offset per char, and per tag name the
    /// last offset of `</name` in it (ASCII case folded), so that typed HTML
    /// without an end tag is recognised without a scan.
    src: std::rc::Rc<str>,
    bytes: Vec<usize>,
    last_end_tag: std::collections::HashMap<String, Option<usize>>,
    /// The closer searches over the text, and the ends of typed HTML
    /// fragments per tag name and end of range, by byte offset.
    scan: Scan,
    fragments:
        std::collections::HashMap<(String, usize), std::collections::HashMap<usize, Option<usize>>>,
    /// Elements open around what is being read. From `html::MAX_DEPTH` on
    /// no construct is taken: it stays text.
    depth: usize,
    /// Whether nesting beyond the cap has been reported.
    pub too_deep: bool,
}

pub(crate) fn element(name: &str, attrs: Vec<Attr>, children: Vec<Node>) -> Node {
    Node::Element(Element {
        name: name.to_string(),
        attrs,
        children,
        span: 0..0,
        implied_end: false,
    })
}

pub(crate) fn text(s: impl Into<String>) -> Node {
    Node::Text(Text {
        text: s.into(),
        span: 0..0,
    })
}

/// The element and the attributes a selector names.
pub(crate) fn from_selector(sel: &Selector) -> (String, Vec<Attr>) {
    let mut attrs = Vec::new();
    if let Some(id) = &sel.id {
        attrs.push(Attr {
            name: "id".into(),
            value: id.clone(),
        });
    }
    let mut classes = sel.classes.clone();
    for (name, value) in &sel.attrs {
        match name.as_str() {
            "class" => classes.extend(value.split_whitespace().map(str::to_string)),
            "id" if sel.id.is_some() => {}
            _ => attrs.push(Attr {
                name: name.clone(),
                value: value.clone(),
            }),
        }
    }
    if !classes.is_empty() {
        attrs.push(Attr {
            name: "class".into(),
            value: classes.join(" "),
        });
    }
    (sel.name.clone(), attrs)
}

pub(crate) fn is_void(name: &str) -> bool {
    matches!(name, "br" | "img" | "hr")
}

/// The markers that may begin with `ch`, longest first, with their elements.
fn markers(ch: char) -> &'static [(&'static str, &'static str)] {
    match ch {
        '*' => &[("**", "strong"), ("*", "em")],
        '_' => &[("__", "strong"), ("_", "em")],
        '=' => &[("==", "mark")],
        '~' => &[("~~", "s"), ("~", "sub")],
        '^' => &[("^", "sup")],
        _ => &[],
    }
}

/// Whether the marker `m` at `j` could close one (§5.5.2).
pub(crate) fn closes_at(c: &[char], m: &str, j: usize, end: usize) -> bool {
    let n = m.chars().count();
    let intraword = !matches!(m, "^" | "~");
    starts(c, j, end, m)
        && !is_space(c[j - 1])
        && !(intraword && c.get(j + n).is_some_and(|x| x.is_alphanumeric()))
}

fn starts(c: &[char], i: usize, end: usize, m: &str) -> bool {
    let n = m.chars().count();
    i + n <= end && c[i..i + n].iter().copied().eq(m.chars())
}

fn is_space(ch: char) -> bool {
    ch.is_whitespace()
}

impl<'p> Inline<'p> {
    pub(crate) fn new(profile: &'p Profile) -> Self {
        Self::within(profile, 0, false)
    }

    /// A reader for the content of an element open at `depth`.
    pub(crate) fn within(profile: &'p Profile, depth: usize, too_deep: bool) -> Self {
        Inline {
            profile,
            diags: Vec::new(),
            src: "".into(),
            bytes: Vec::new(),
            last_end_tag: std::collections::HashMap::new(),
            scan: Scan::new(&[]),
            fragments: std::collections::HashMap::new(),
            depth,
            too_deep,
        }
    }

    /// Reads the content of an element that opens inside the current one.
    fn inside(&mut self, c: &[char], start: usize, end: usize) -> Vec<Node> {
        self.depth += 1;
        let out = self.range(c, start, end);
        self.depth -= 1;
        out
    }

    pub(crate) fn read(&mut self, s: &str) -> Vec<Node> {
        let c: Vec<char> = s.chars().collect();
        self.src = s.into();
        self.bytes = s
            .char_indices()
            .map(|(b, _)| b)
            .chain(std::iter::once(s.len()))
            .collect();
        self.last_end_tag.clear();
        self.scan = Scan::new(&c);
        self.fragments.clear();
        self.range(&c, 0, c.len())
    }

    /// Whether `</name` occurs at or after byte `from`.
    fn end_tag_after(&mut self, name: &str, from: usize) -> bool {
        let src = self.src.clone();
        let last = *self
            .last_end_tag
            .entry(name.to_string())
            .or_insert_with(|| {
                let needle = format!("</{name}");
                src.to_ascii_lowercase().rfind(&needle)
            });
        last.is_some_and(|l| l >= from)
    }

    fn range(&mut self, c: &[char], start: usize, end: usize) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        let mut buf = String::new();
        let flush = |buf: &mut String, out: &mut Vec<Node>| {
            if !buf.is_empty() {
                out.push(text(std::mem::take(buf)));
            }
        };
        let mut i = start;
        while i < end {
            let ch = c[i];
            if ch == '\\' && i + 1 < end && c[i + 1].is_ascii_punctuation() {
                buf.push(c[i + 1]);
                i += 2;
                continue;
            }
            if ch == '\n' {
                flush(&mut buf, &mut out);
                out.push(element("br", Vec::new(), Vec::new()));
                i += 1;
                continue;
            }
            if self.depth >= html::MAX_DEPTH {
                if !self.too_deep && construct_at(self.profile, &mut self.scan, c, i, end).is_some()
                {
                    self.too_deep = true;
                    self.diags.push(html::too_deep());
                }
                buf.push(ch);
                i += 1;
                continue;
            }
            if let Some((node, next)) = self.construct(c, i, end) {
                flush(&mut buf, &mut out);
                if let Some(node) = node {
                    out.push(node)
                }
                i = next;
                continue;
            }
            if ch == '`' {
                // A run without a partner is literal, all of it.
                while i < end && c[i] == '`' {
                    buf.push('`');
                    i += 1;
                }
                continue;
            }
            buf.push(ch);
            i += 1;
        }
        flush(&mut buf, &mut out);
        out
    }

    /// A construct beginning at `i`: the node it produces (None for one that
    /// produces nothing) and the index after it.
    fn construct(&mut self, c: &[char], i: usize, end: usize) -> Option<(Option<Node>, usize)> {
        match c[i] {
            '`' => {
                let (content, next) = self.scan.code_span(c, i, end)?;
                Some((Some(element("code", Vec::new(), vec![text(content)])), next))
            }
            '[' => self.link(c, i, end).map(|(n, next)| (Some(n), next)),
            '.' => self.long_form(c, i, end).map(|(n, next)| (Some(n), next)),
            '<' => self.typed_html(c, i, end),
            '*' | '_' | '=' | '~' | '^' => self.marker(c, i, end).map(|(n, next)| (Some(n), next)),
            _ => None,
        }
    }

    /// §5.5.2: tries the markers at `i`, longest first.
    fn marker(&mut self, c: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
        let (m, name, close) = find_marker(&mut self.scan, c, i, end)?;
        let n = m.chars().count();
        let children = self.inside(c, i + n, close);
        Some((element(name, Vec::new(), children), close + n))
    }

    /// §5.5.4: `[text](path)`.
    fn link(&mut self, c: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
        let (close, path, next) = find_link(&mut self.scan, c, i, end)?;
        let children = self.inside(c, i + 1, close);
        let href = url::canonical(&path);
        Some((
            element(
                "a",
                vec![Attr {
                    name: "href".into(),
                    value: href,
                }],
                children,
            ),
            next,
        ))
    }

    /// §5.5.1: `.element#id.class[attr="value"](content)`, for inline elements.
    fn long_form(&mut self, c: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
        let shape = long_form_shape(self.profile, &mut self.scan, c, i, end)?;
        let sel = syntax::build(c, &shape);
        let (name, attrs) = from_selector(&sel);
        let children = match sel.content {
            Some((a, b)) => self.inside(c, a, b),
            None => Vec::new(),
        };
        Some((element(&name, attrs, children), sel.end))
    }

    /// §6 typed HTML and §5.11 comments inside phrasing.
    fn typed_html(&mut self, c: &[char], i: usize, end: usize) -> Option<(Option<Node>, usize)> {
        let src = self.src.clone();
        let (a, b) = (*self.bytes.get(i)?, *self.bytes.get(end)?);
        let rest: &str = src.get(a..b)?;
        if let Some(body) = rest.strip_prefix("<!--") {
            return match body.find("-->") {
                Some(k) => {
                    let inner = &body[..k];
                    self.diags
                        .push(Diagnostic::new(Code::Comment, inner.trim().to_string()));
                    let len = 4 + inner.chars().count() + 3;
                    Some((
                        Some(Node::Comment(Comment {
                            text: inner.to_string(),
                            span: 0..0,
                        })),
                        i + len,
                    ))
                }
                None => {
                    self.diags
                        .push(Diagnostic::new(Code::UnclosedComment, "<!-- without -->"));
                    // The rest stays text.
                    Some((Some(text(rest)), end))
                }
            };
        }
        let next = *c.get(i + 1)?;
        if !(next.is_ascii_alphabetic() || next == '/') {
            return None;
        }
        let name: String = c[i + 1..end]
            .iter()
            .take_while(|x| x.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let self_closing = rest.find('>').is_some_and(|k| rest[..k].ends_with('/'));
        let closable = is_void(&name) || self_closing || self.end_tag_after(&name, a);
        let taken = if next == '/' || !closable {
            None
        } else {
            let ends = self.fragments.entry((name.clone(), b)).or_default();
            html::fragment_len_in(rest, a, ends).map(|len| {
                let fragment = &rest[..len];
                let nodes = html::parse(fragment);
                (fragment.chars().count(), nodes)
            })
        };
        match taken {
            Some((len, nodes))
                if !self
                    .profile
                    .diagnose(&nodes, true)
                    .iter()
                    .any(|d| d.code.is_blocking())
                    && nodes.iter().all(|n| match n {
                        Node::Element(e) => self.profile.kind(&e.name) == Some(Kind::Inline),
                        _ => true,
                    }) =>
            {
                // Taken as a whole: no notation inside.
                Some((
                    Some(match nodes.len() {
                        1 => nodes.into_iter().next().unwrap_or_else(|| text("")),
                        _ => element("#fragment", Vec::new(), nodes),
                    }),
                    i + len,
                ))
            }
            other => {
                let len = match other {
                    Some((len, _)) => len,
                    None => tag_len(rest).unwrap_or(1),
                };
                self.diags.push(Diagnostic::new(
                    Code::HtmlNotTaken,
                    c[i..i + len].iter().collect::<String>(),
                ));
                Some((
                    Some(text(c[i..i + len].iter().collect::<String>())),
                    i + len,
                ))
            }
        }
    }
}

/// The length in chars of the tag at the start of `s`, if it is closed.
fn tag_len(s: &str) -> Option<usize> {
    let close = s.find('>')?;
    Some(s[..=close].chars().count())
}

/// A long form at `i` naming an inline element of the profile, with a
/// bracket (§5.5.1). A void element takes no content.
pub(crate) fn long_form_shape(
    profile: &Profile,
    scan: &mut Scan,
    c: &[char],
    i: usize,
    end: usize,
) -> Option<Shape> {
    let shape = syntax::shape(scan, c, i, end)?;
    let name = shape.name(c);
    if profile.kind(&name) != Some(Kind::Inline) {
        return None;
    }
    if !shape.has_attr_bracket() && shape.content.is_none() {
        return None;
    }
    if is_void(&name) && shape.content.is_some_and(|(a, b)| a != b) {
        return None;
    }
    Some(shape)
}

/// The marker that opens at `i` and the index where it closes (§5.5.2).
pub(crate) fn find_marker(
    scan: &mut Scan,
    c: &[char],
    i: usize,
    end: usize,
) -> Option<(&'static str, &'static str, usize)> {
    for &(m, name) in markers(c[i]) {
        if !starts(c, i, end, m) {
            continue;
        }
        let n = m.chars().count();
        let intraword = !matches!(m, "^" | "~");
        let Some(&next) = c.get(i + n).filter(|_| i + n < end) else {
            continue;
        };
        if is_space(next) {
            continue;
        }
        if intraword && i > 0 && c[i - 1].is_alphanumeric() {
            continue;
        }
        // A closer right at the start would leave the content empty.
        let from = i + n;
        let skip = usize::from(closes_at(c, m, from, end));
        let close = scan.marker_close(c, m, from + skip, end);
        let Some(j) = close else {
            continue;
        };
        if intraword || !scan.space_in(from, j) {
            return Some((m, name, j));
        }
    }
    None
}

/// A link at `c[i] == '['`: the index of `]`, of the path's first character
/// and of the `)` after it.
fn link_bounds(scan: &mut Scan, c: &[char], i: usize, end: usize) -> Option<(usize, usize, usize)> {
    let close = scan.bracket_close(c, i, end)?;
    if c.get(close + 1) != Some(&'(') {
        return None;
    }
    let from = close + 2;
    Some((close, from, scan.path_close(c, from, end)?))
}

/// A link at `c[i] == '['`: the index of `]`, the path, the index after.
pub(crate) fn find_link(
    scan: &mut Scan,
    c: &[char],
    i: usize,
    end: usize,
) -> Option<(usize, String, usize)> {
    let (close, from, to) = link_bounds(scan, c, i, end)?;
    let mut path = String::new();
    let mut k = from;
    while k < to {
        if c[k] == '\\' && k + 1 < end && matches!(c[k + 1], ')' | '\\') {
            path.push(c[k + 1]);
            k += 2;
        } else {
            path.push(c[k]);
            k += 1;
        }
    }
    Some((close, path, to + 1))
}

/// W2: if the reader would begin a construct at `i`, the index of the
/// character to put a backslash before.
pub(crate) fn construct_at(
    profile: &Profile,
    scan: &mut Scan,
    c: &[char],
    i: usize,
    end: usize,
) -> Option<usize> {
    match c[i] {
        '\\' if i + 1 < end && c[i + 1].is_ascii_punctuation() => Some(i),
        '`' => scan.code_span_end(i, end).map(|_| i),
        '[' => link_bounds(scan, c, i, end).map(|_| i),
        '.' => long_form_shape(profile, scan, c, i, end).map(|_| i),
        '<' => c
            .get(i + 1)
            .filter(|&&x| x.is_ascii_alphabetic() || x == '/' || x == '!')
            .map(|_| i),
        '*' | '_' | '=' | '~' | '^' => find_marker(scan, c, i, end).map(|_| i),
        _ => None,
    }
}

/// Typed fragments of several nodes come back wrapped; this unwraps them.
pub(crate) fn unwrap_fragments(nodes: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::Element(e) if e.name == "#fragment" => out.extend(unwrap_fragments(e.children)),
            Node::Element(mut e) => {
                e.children = unwrap_fragments(e.children);
                out.push(Node::Element(e));
            }
            other => out.push(other),
        }
    }
    out
}
