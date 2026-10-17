//! What the obsidian flavour adds to GFM (`docs/converters.md` §2):
//! frontmatter, callouts, highlights, comments and tags. Wikilinks and
//! embeds are read by pulldown-cmark and resolved in the builder.

use lipupona::dom::{Comment, Element, Node, Text};

use crate::{Entry, Meta};

fn text(s: impl Into<String>) -> Node {
    Node::Text(Text {
        text: s.into(),
        span: 0..0,
    })
}

/// YAML between `---` lines at the very start, and the Markdown after it.
pub(crate) fn split_frontmatter(md: &str) -> Option<(&str, &str)> {
    let rest = md
        .strip_prefix("---\n")
        .or_else(|| md.strip_prefix("---\r\n"))?;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == "---" || bare == "..." {
            return Some((&rest[..at], &rest[at + line.len()..]));
        }
        at += line.len();
    }
    None
}

/// Reads flat keys with a scalar or a list (`[a, b]`, or `- a` lines below
/// the key); anything else is kept as written.
pub(crate) fn frontmatter(yaml: &str, meta: &mut Meta, report: &mut Vec<Entry>) {
    let mut keys: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indented = line.starts_with([' ', '\t']) || trimmed.starts_with("- ");
        if indented && let Some((_, raw, items)) = keys.last_mut() {
            match trimmed.strip_prefix('-') {
                Some(item) => items.push(unquote(item.trim()).to_string()),
                None => {
                    if !raw.is_empty() {
                        raw.push('\n');
                    }
                    raw.push_str(trimmed);
                }
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let items = match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Some(inner) => inner
                .split(',')
                .map(|v| unquote(v.trim()).to_string())
                .filter(|v| !v.is_empty())
                .collect(),
            None => Vec::new(),
        };
        let raw = if items.is_empty() {
            unquote(value).to_string()
        } else {
            String::new()
        };
        keys.push((key.trim().to_string(), raw, items));
    }
    for (key, raw, items) in keys {
        let scalar = || match items.is_empty() {
            true => (!raw.is_empty()).then(|| raw.clone()),
            false => Some(items.join(", ")),
        };
        let list = || match items.is_empty() {
            true => raw
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect(),
            false => items.clone(),
        };
        match key.as_str() {
            "title" => meta.title = scalar(),
            "lang" => meta.lang = scalar(),
            "created" => meta.created = scalar(),
            "source" => meta.source = scalar(),
            "aliases" => meta.aliases = list(),
            "tags" => {
                for tag in list() {
                    let tag = tag.trim_start_matches('#').to_string();
                    if !tag.is_empty() && !meta.tags.contains(&tag) {
                        meta.tags.push(tag);
                    }
                }
            }
            _ => {
                report.push(Entry {
                    code: "unknown-frontmatter-key".into(),
                    message: key.clone(),
                });
                if let Some(value) = scalar() {
                    meta.other.push((key, value));
                }
            }
        }
    }
}

fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// A blockquote that opens with `[!type]`, `[!type]-` or `[!type]+` and an
/// optional title on its first line: type, folding, title and content.
pub(crate) type Callout = (String, Option<char>, Option<Vec<Node>>, Vec<Node>);

pub(crate) fn callout(quote: &Element) -> Option<Callout> {
    let Some(Node::Element(p)) = quote.children.first() else {
        return None;
    };
    if p.name != "p" || !p.attrs.is_empty() {
        return None;
    }
    let Some(Node::Text(first)) = p.children.first() else {
        return None;
    };
    let rest = first.text.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = &rest[..close];
    let mut chars = kind.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let mut after = &rest[close + 1..];
    let fold = match after.chars().next() {
        Some(c @ ('-' | '+')) => {
            after = &after[1..];
            Some(c)
        }
        _ => None,
    };
    if !(after.is_empty() || after.starts_with(' ')) {
        return None;
    }
    // The title runs to the first line break of the paragraph.
    let mut title = Vec::new();
    if !after.trim().is_empty() {
        title.push(text(after.trim_start()));
    }
    let mut nodes = p.children[1..].iter().cloned();
    let mut rest_of_p = Vec::new();
    for node in nodes.by_ref() {
        if matches!(&node, Node::Element(e) if e.name == "br") {
            rest_of_p.extend(nodes.by_ref());
            break;
        }
        title.push(node);
    }
    trim_end(&mut title);
    let title = (!title.is_empty()).then_some(title);
    let mut content = Vec::new();
    if !rest_of_p.is_empty() {
        let mut p2 = p.clone();
        p2.children = rest_of_p;
        content.push(Node::Element(p2));
    }
    content.extend(quote.children[1..].iter().cloned());
    Some((kind.to_ascii_lowercase(), fold, title, content))
}

fn trim_end(nodes: &mut Vec<Node>) {
    if let Some(Node::Text(t)) = nodes.last_mut() {
        let trimmed = t.text.trim_end().len();
        t.text.truncate(trimmed);
        if t.text.is_empty() {
            nodes.pop();
        }
    }
}

/// Comments `%%…%%` and highlights `==…==` in every run of phrasing,
/// outside code.
pub(crate) fn inline(nodes: &mut Vec<Node>) {
    pair(nodes, "%%", Wrap::Comment);
    pair(nodes, "==", Wrap::Mark);
    for node in nodes.iter_mut() {
        if let Node::Element(e) = node
            && e.name != "code"
            && e.name != "pre"
        {
            inline(&mut e.children);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wrap {
    Comment,
    Mark,
}

enum Piece {
    Node(Node),
    Marker,
}

/// Pairs the markers in the text of `nodes` from left to right and wraps
/// what lies between each pair.
fn pair(nodes: &mut Vec<Node>, marker: &str, wrap: Wrap) {
    if !nodes
        .iter()
        .any(|n| matches!(n, Node::Text(t) if t.text.contains(marker)))
    {
        return;
    }
    let mut pieces = Vec::new();
    for node in std::mem::take(nodes) {
        let Node::Text(t) = node else {
            pieces.push(Piece::Node(node));
            continue;
        };
        let mut rest = t.text.as_str();
        while let Some(k) = rest.find(marker) {
            if k > 0 {
                pieces.push(Piece::Node(text(&rest[..k])));
            }
            pieces.push(Piece::Marker);
            rest = &rest[k + marker.len()..];
        }
        if !rest.is_empty() {
            pieces.push(Piece::Node(text(rest)));
        }
    }
    let side = |p: Option<&Piece>, last: bool| -> Option<char> {
        match p? {
            Piece::Marker => marker.chars().next(),
            Piece::Node(Node::Text(t)) if last => t.text.chars().last(),
            Piece::Node(Node::Text(t)) => t.text.chars().next(),
            Piece::Node(_) => Some('x'),
        }
    };
    // Whether each marker may open and may close.
    let flags: Vec<(bool, bool)> = (0..pieces.len())
        .map(|k| {
            let before = side(k.checked_sub(1).and_then(|j| pieces.get(j)), true);
            let after = side(pieces.get(k + 1), false);
            match wrap {
                Wrap::Comment => (true, true),
                Wrap::Mark => (
                    after.is_some_and(|c| !c.is_whitespace()),
                    before.is_some_and(|c| !c.is_whitespace()),
                ),
            }
        })
        .collect();
    let mut out: Vec<Node> = Vec::new();
    // The index in `out` of the open marker, kept as text until it closes.
    let mut open: Option<usize> = None;
    for (piece, (can_open, can_close)) in pieces.into_iter().zip(flags) {
        let Piece::Marker = piece else {
            if let Piece::Node(n) = piece {
                out.push(n);
            }
            continue;
        };
        match open {
            Some(at) if can_close && out.len() > at + 1 => {
                let inner: Vec<Node> = out.drain(at + 1..).collect();
                out.pop();
                out.push(match wrap {
                    Wrap::Mark => Node::Element(Element {
                        name: "mark".into(),
                        attrs: Vec::new(),
                        children: inner,
                        span: 0..0,
                        implied_end: false,
                    }),
                    Wrap::Comment => Node::Comment(Comment {
                        text: plain(&inner),
                        span: 0..0,
                    }),
                });
                open = None;
            }
            None if can_open => {
                out.push(text(marker));
                open = Some(out.len() - 1);
            }
            _ => out.push(text(marker)),
        }
    }
    *nodes = merge_text(out);
}

fn plain(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.text),
            Node::Element(e) if e.name == "br" => out.push('\n'),
            Node::Element(e) => out.push_str(&plain(&e.children)),
            _ => {}
        }
    }
    out
}

fn merge_text(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let (Node::Text(t), Some(Node::Text(last))) = (&node, out.last_mut()) {
            last.text.push_str(&t.text);
            continue;
        }
        out.push(node);
    }
    out
}

/// Tags `#tag` in the text, outside code and links, in order of first
/// occurrence, added to `tags`.
pub(crate) fn tags(nodes: &[Node], tags: &mut Vec<String>) {
    let mut prev_break = true;
    for node in nodes {
        match node {
            Node::Text(t) => {
                let chars: Vec<char> = t.text.chars().collect();
                for (k, &c) in chars.iter().enumerate() {
                    if c != '#' {
                        continue;
                    }
                    let starts = match k {
                        0 => prev_break,
                        _ => chars[k - 1].is_whitespace(),
                    };
                    if !starts {
                        continue;
                    }
                    let tag: String = chars[k + 1..]
                        .iter()
                        .take_while(|&&x| x.is_alphanumeric() || "_-/".contains(x))
                        .collect();
                    if !tag.is_empty()
                        && !tag.chars().all(|x| x.is_ascii_digit())
                        && !tags.contains(&tag)
                    {
                        tags.push(tag);
                    }
                }
                prev_break = t.text.ends_with(char::is_whitespace);
            }
            Node::Element(e) => {
                if !matches!(e.name.as_str(), "code" | "pre" | "a") {
                    self::tags(&e.children, tags);
                }
                prev_break = e.name == "br" || !is_phrasing(&e.name);
            }
            _ => prev_break = false,
        }
    }
}

fn is_phrasing(name: &str) -> bool {
    matches!(
        name,
        "strong"
            | "em"
            | "mark"
            | "code"
            | "s"
            | "sup"
            | "sub"
            | "kbd"
            | "a"
            | "span"
            | "time"
            | "img"
    )
}
