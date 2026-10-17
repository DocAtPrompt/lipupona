//! Reading notation: the line model (SPEC §5.2) and every block construct
//! of §5 and §6.

use super::inline::{self, Inline, element, from_selector, text};
use super::syntax;
use super::table;
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Element, Node};
use crate::html;
use crate::profile::{Kind, Model, Profile};

pub(crate) struct Reader<'p> {
    pub profile: &'p Profile,
    pub diags: Vec<Diagnostic>,
    /// Elements open around what is being read. From `html::MAX_DEPTH` on
    /// nothing more is opened: what would open an element is text.
    depth: usize,
    /// Whether the top-level block being read would be deeper than the cap
    /// (SPEC §3.4).
    too_deep: bool,
}

/// The text lines of the paragraph being collected: as the inline reader
/// reads them, and as they were typed.
#[derive(Default)]
struct Para {
    lines: Vec<String>,
    typed: Vec<String>,
}

impl Para {
    fn push(&mut self, line: String) {
        self.typed.push(line.clone());
        self.lines.push(line);
    }

    /// A line of literal text: `line` reads as `typed`.
    fn push_literal(&mut self, line: String, typed: String) {
        self.lines.push(line);
        self.typed.push(typed);
    }

    fn pop(&mut self) -> Option<(String, String)> {
        Some((self.lines.pop()?, self.typed.pop()?))
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn len(&self) -> usize {
        self.lines.len()
    }
}

/// The depth of the deepest element in `nodes`.
fn tree_depth(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Element(e) => 1 + tree_depth(&e.children),
            Node::Verbatim(_) => 1,
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

/// The lines of one context as one text, made when a comment or typed HTML
/// block may begin, so that each such line does not join the rest again.
/// A line the reader shortens (to what follows a typed block on it) stays a
/// suffix of its line here.
struct Joined {
    text: String,
    /// Byte offset of each line.
    starts: Vec<usize>,
    /// Where `-->` begins, in order.
    dashes: Vec<usize>,
    /// Ends of typed fragments per tag name (`html::fragment_len_in`).
    ends: std::collections::HashMap<String, std::collections::HashMap<usize, Option<usize>>>,
}

impl Joined {
    fn new(lines: &[String]) -> Self {
        let text = lines.join("\n");
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for line in lines {
            starts.push(at);
            at += line.len() + 1;
        }
        let dashes = text.match_indices("-->").map(|(k, _)| k).collect();
        Joined {
            text,
            starts,
            dashes,
            ends: std::collections::HashMap::new(),
        }
    }

    /// The offset where `line`, the current form of line `i`, begins
    /// without its indentation.
    fn from(&self, i: usize, line: &str) -> usize {
        let end = self.starts.get(i + 1).map_or(self.text.len(), |&s| s - 1);
        end - line.trim_start_matches(SP).len()
    }
}

/// A comment or typed HTML block found at a line, before it is taken.
enum Found {
    Comment(String, usize),
    Typed(Vec<Node>, usize, Option<String>),
    /// A typed block that is closed but not taken (§6): its lines, as the
    /// inline reader reads them (nothing in them is notation, then what
    /// follows its end tag) and as typed; and the diagnostic.
    Literal(Vec<(String, String)>, Diagnostic),
}

/// Lines `from..to` of a context without their indentation.
fn input_lines(lines: &[String], from: usize, to: usize) -> Vec<String> {
    lines[from..to.min(lines.len())]
        .iter()
        .map(|l| l.trim_start_matches(SP).to_string())
        .collect()
}

/// Text that the inline reader reads as it is written: a backslash before
/// every ASCII punctuation character (§5.12).
fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// What a line opens (§5.2), seen without its indentation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Opener {
    Fence {
        ch: char,
        len: usize,
        info: String,
    },
    Comment,
    TypedBlock,
    Container {
        len: usize,
        spec: String,
    },
    AttrLine(Vec<(String, String)>),
    Heading {
        level: usize,
        text: String,
    },
    Hr,
    Quote,
    Grid,
    CellTable,
    Item {
        number: Option<u64>,
        content: String,
    },
    ElementLine(syntax::Selector),
}

/// The whitespace of indentation and of line ends in notation. Other
/// whitespace — a no-break space — is content.
pub(crate) const SP: [char; 2] = [' ', '\t'];

/// Width of the indentation, a tab counting as one level (two spaces).
pub(crate) fn indent_width(line: &str) -> usize {
    let mut w = 0;
    for ch in line.chars() {
        match ch {
            ' ' => w += 1,
            '\t' => w += 2,
            _ => break,
        }
    }
    w
}

/// Removes up to `width` columns of indentation.
pub(crate) fn dedent(line: &str, width: usize) -> String {
    let mut w = 0;
    let mut cut = 0;
    for (i, ch) in line.char_indices() {
        if w >= width {
            break;
        }
        match ch {
            ' ' => w += 1,
            '\t' => w += 2,
            _ => break,
        }
        cut = i + ch.len_utf8();
    }
    line[cut..].to_string()
}

pub(crate) fn is_blank(line: &str) -> bool {
    line.trim_matches(SP).is_empty()
}

/// What `rest` (a line without indentation) opens; None for a text line.
/// `figure` admits an `img` element line (§5.4.4).
pub(crate) fn opener(profile: &Profile, rest: &str, figure: bool) -> Option<Opener> {
    let c: Vec<char> = rest.chars().collect();
    let first = *c.first()?;
    let run = |ch: char| c.iter().take_while(|&&x| x == ch).count();
    match first {
        '`' | '~' if run(first) >= 3 => {
            let len = run(first);
            let info: String = c[len..]
                .iter()
                .collect::<String>()
                .trim_matches(SP)
                .to_string();
            if first == '`' && info.contains('`') {
                return None;
            }
            Some(Opener::Fence {
                ch: first,
                len,
                info,
            })
        }
        // A comment is a block when nothing follows it on its last line.
        '<' if rest.starts_with("<!--") => match rest[4..].find("-->") {
            Some(k) => rest[4 + k + 3..]
                .trim_matches(SP)
                .is_empty()
                .then_some(Opener::Comment),
            None => Some(Opener::Comment),
        },
        '<' => {
            let name: String = c[1..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            let after = c.get(1 + name.len()).copied();
            let boundary = after.is_none_or(|x| x == '>' || x == '/' || x.is_whitespace());
            (!name.is_empty() && boundary && profile.kind(&name) == Some(Kind::Block))
                .then_some(Opener::TypedBlock)
        }
        ':' if run(':') >= 3 => {
            let len = run(':');
            Some(Opener::Container {
                len,
                spec: c[len..]
                    .iter()
                    .collect::<String>()
                    .trim_matches(SP)
                    .to_string(),
            })
        }
        '{' => syntax::brace_attributes(rest).map(Opener::AttrLine),
        '#' => {
            let level = run('#');
            match c.get(level) {
                Some(' ') => Some(Opener::Heading {
                    level,
                    text: c[level + 1..]
                        .iter()
                        .collect::<String>()
                        .trim_matches(SP)
                        .to_string(),
                }),
                _ => None,
            }
        }
        '-' if rest.trim_end_matches(SP) == "---" => Some(Opener::Hr),
        '-' if c.len() == 1 || c.get(1) == Some(&' ') => Some(Opener::Item {
            number: None,
            content: c.get(2..).map(|r| r.iter().collect()).unwrap_or_default(),
        }),
        '>' => Some(Opener::Quote),
        '|' if rest.starts_with("|===") => {
            let count = rest[4..].trim_matches(SP);
            (!count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
                .then_some(Opener::CellTable)
        }
        '|' => Some(Opener::Grid),
        '0'..='9' => {
            let digits = c.iter().take_while(|x| x.is_ascii_digit()).count();
            if c.get(digits) != Some(&'.') {
                return None;
            }
            match c.get(digits + 1) {
                None | Some(' ') => {
                    let number: String = c[..digits].iter().collect();
                    Some(Opener::Item {
                        number: Some(number.parse().unwrap_or(u64::MAX)),
                        content: c
                            .get(digits + 2..)
                            .map(|r| r.iter().collect())
                            .unwrap_or_default(),
                    })
                }
                _ => None,
            }
        }
        '.' => {
            let sel = syntax::selector(&c, 0, c.len())?;
            let tail: String = c[sel.end..].iter().collect();
            if !tail.trim_matches(SP).is_empty() {
                return None;
            }
            let (kind, model) = profile.element(&sel.name)?;
            if model == Model::Void && sel.content.is_some_and(|(a, b)| a != b) {
                return None;
            }
            match kind {
                Kind::Block => Some(Opener::ElementLine(sel)),
                Kind::Inline if figure && sel.name == "img" => Some(Opener::ElementLine(sel)),
                Kind::Inline => None,
            }
        }
        _ => None,
    }
}

impl<'p> Reader<'p> {
    pub(crate) fn new(profile: &'p Profile) -> Self {
        Reader {
            profile,
            diags: Vec::new(),
            depth: 0,
            too_deep: false,
        }
    }

    /// Reads with `levels` more elements open around the content.
    pub(crate) fn inside<T>(&mut self, levels: usize, read: impl FnOnce(&mut Self) -> T) -> T {
        self.depth += levels;
        let out = read(self);
        self.depth -= levels;
        out
    }

    /// The phrasing content of an element open at the current depth.
    pub(crate) fn phrasing(&mut self, s: &str) -> Vec<Node> {
        let mut r = Inline::within(self.profile, self.depth, self.too_deep);
        let nodes = inline::unwrap_fragments(r.read(s));
        self.too_deep = r.too_deep;
        self.diags.append(&mut r.diags);
        nodes
    }

    fn paragraph(&mut self, lines: &[String]) -> Node {
        let text: Vec<&str> = lines.iter().map(|l| l.trim_start_matches(SP)).collect();
        let children = self.inside(1, |r| r.phrasing(&text.join("\n")));
        element("p", Vec::new(), children)
    }

    /// Reads the lines of one context, already without its indentation.
    pub(crate) fn blocks(&mut self, input: &[String], figure: bool) -> Vec<Node> {
        let mut lines: Vec<String> = input.to_vec();
        let capped = self.depth >= html::MAX_DEPTH;
        if capped && lines.iter().any(|l| !is_blank(l)) {
            self.too_deep = true;
        }
        let mut out: Vec<Node> = Vec::new();
        let mut para = Para::default();
        let mut pending: Option<Vec<(String, String)>> = None;
        let mut joined: Option<Joined> = None;
        let mut i = 0;
        while i < lines.len() {
            if is_blank(&lines[i]) {
                self.flush(&mut para, &mut out, &mut pending);
                i += 1;
                continue;
            }
            let rest = lines[i].trim_start_matches(SP).to_string();
            let op = (!capped)
                .then(|| opener(self.profile, &rest, figure))
                .flatten();
            let Some(op) = op else {
                para.push(rest);
                i += 1;
                continue;
            };
            if let Opener::AttrLine(attrs) = &op {
                let next_is_block = lines.get(i + 1).is_some_and(|l| !is_blank(l));
                if para.is_empty() && next_is_block {
                    let mut merged = pending.take().unwrap_or_default();
                    merged.extend(attrs.iter().cloned());
                    pending = Some(merged);
                    i += 1;
                    continue;
                }
                para.push(rest);
                i += 1;
                continue;
            }
            // What only looks like a comment or typed HTML block is text.
            let found = match &op {
                Opener::Comment | Opener::TypedBlock => {
                    let joined = joined.get_or_insert_with(|| Joined::new(input));
                    match op {
                        Opener::Comment => self.block_comment(joined, &lines, i),
                        _ => self.typed_block(joined, &lines, i),
                    }
                }
                _ => None,
            };
            if let Some(Found::Literal(text_lines, diag)) = found {
                // Text lines, which a blank line inside divides (§5.2).
                self.diags.push(diag);
                i += text_lines.len();
                for (line, typed) in text_lines {
                    if is_blank(&typed) {
                        self.flush(&mut para, &mut out, &mut pending);
                    } else {
                        para.push_literal(line, typed);
                    }
                }
                continue;
            }
            let taken = match &op {
                Opener::Comment | Opener::TypedBlock => found.is_some(),
                Opener::Container { spec, .. } => self.container(spec).is_some(),
                _ => true,
            };
            if !taken {
                para.push(rest);
                i += 1;
                continue;
            }
            let caption = if matches!(op, Opener::Grid | Opener::CellTable) && para.len() == 1 {
                para.pop()
            } else {
                self.flush(&mut para, &mut out, &mut pending);
                None
            };
            let caption_typed = caption.as_ref().map(|(_, typed)| typed.clone());
            let caption = caption.map(|(line, _)| line);
            let before = out.len();
            let line_before = i;
            let top = self.depth == 0;
            if top {
                self.too_deep = false;
            }
            i = match found {
                Some(found) => self.take_found(found, &mut lines, i, &mut out),
                None => self.take(op, &mut lines, i, &rest, caption, &mut out),
            };
            // A top-level block that would be too deep stays text as a
            // whole (§3.4): its lines, as typed, are text lines.
            if top && (self.too_deep || tree_depth(&out[before..]) > html::MAX_DEPTH) {
                out.truncate(before);
                self.diags.push(html::too_deep());
                let typed = caption_typed
                    .into_iter()
                    .chain(input_lines(&lines, line_before, i));
                for line in typed {
                    if is_blank(&line) {
                        self.flush(&mut para, &mut out, &mut pending);
                    } else {
                        para.push_literal(literal(&line), line);
                    }
                }
                continue;
            }
            if let Some(attrs) = pending.take()
                && let Some(Node::Element(e)) = out.get_mut(before)
            {
                self.merge(e, attrs);
            }
            // Every line is read once: a construct that took nothing leaves
            // its line as text.
            if i == line_before {
                out.truncate(before);
                para.push(lines[i].trim_start_matches(SP).to_string());
                i += 1;
            }
        }
        self.flush(&mut para, &mut out, &mut pending);
        out
    }

    /// Reads the construct `op` opens at `lines[i]` into `out`; returns the
    /// index of the next line. Every construct that nests has a method of its
    /// own, so that a level of nesting costs little stack.
    fn take(
        &mut self,
        op: Opener,
        lines: &mut [String],
        i: usize,
        rest: &str,
        caption: Option<String>,
        out: &mut Vec<Node>,
    ) -> usize {
        match op {
            Opener::Fence { ch, len, info } => self.fence(lines, i, ch, len, &info, out),
            // Found before, and taken by `take_found`.
            Opener::Comment | Opener::TypedBlock => i,
            Opener::Container { len, spec } => self.fenced_container(lines, i, len, &spec, out),
            // Taken before; a line that only looks like one stays text.
            Opener::AttrLine(_) => i,
            Opener::Heading { level, text: t } => {
                out.push(self.heading(level, &t));
                i + 1
            }
            Opener::Hr => {
                out.push(element("hr", Vec::new(), Vec::new()));
                i + 1
            }
            Opener::Quote => self.quote_lines(lines, i, out),
            Opener::Grid => {
                // table, tbody, tr, td
                let (node, next) = self.inside(4, |r| table::grid(r, lines, i, caption));
                out.push(node);
                next
            }
            Opener::CellTable => {
                let (node, next) = self.inside(4, |r| table::cells(r, lines, i, caption));
                out.push(node);
                next
            }
            Opener::Item { number, .. } => {
                let (node, next) = self.list(lines, i, number.is_some());
                out.push(node);
                next
            }
            Opener::ElementLine(sel) => self.element_line(lines, i, rest, &sel, out),
        }
    }

    /// Takes a comment or typed HTML block found at `lines[i]`.
    fn take_found(
        &mut self,
        found: Found,
        lines: &mut [String],
        i: usize,
        out: &mut Vec<Node>,
    ) -> usize {
        match found {
            Found::Comment(inner, used) => {
                self.diags.push(Diagnostic::new(
                    Code::Comment,
                    inner.trim_matches(SP).to_string(),
                ));
                out.push(Node::Comment(crate::dom::Comment {
                    text: inner,
                    span: 0..0,
                }));
                i + used
            }
            Found::Literal(..) => i,
            Found::Typed(nodes, used, remainder) => {
                out.extend(nodes);
                match remainder {
                    Some(r) => {
                        lines[i + used] = r;
                        i + used
                    }
                    None => i + used + 1,
                }
            }
        }
    }

    /// §5.10: a fenced code block, or HTML in a fence.
    fn fence(
        &mut self,
        lines: &[String],
        i: usize,
        ch: char,
        len: usize,
        info: &str,
        out: &mut Vec<Node>,
    ) -> usize {
        let close = (i + 1..lines.len()).find(|&j| {
            let t = lines[j].trim_matches(SP);
            t.chars().count() >= len && t.chars().all(|x| x == ch)
        });
        let end = close.unwrap_or(lines.len());
        let content = lines[i + 1..end].join("\n");
        if info == "{=html}" || info == "=html" {
            let nodes = html::parse(&content);
            let nodes = self.profile.check(&content, nodes, &mut self.diags);
            out.extend(nodes.into_iter().filter(|n| !n.is_blank_text()));
        } else {
            let mut attrs = Vec::new();
            if !info.is_empty() {
                attrs.push(Attr {
                    name: "class".into(),
                    value: info.split_whitespace().collect::<Vec<_>>().join(" "),
                });
            }
            let children = if content.is_empty() {
                Vec::new()
            } else {
                vec![text(content)]
            };
            out.push(element("pre", attrs, children));
        }
        close.map_or(end, |j| j + 1)
    }

    /// A fenced container (§5.4, §7.3) at `lines[i]`.
    fn fenced_container(
        &mut self,
        lines: &[String],
        i: usize,
        len: usize,
        spec: &str,
        out: &mut Vec<Node>,
    ) -> usize {
        let mut stack: Vec<usize> = Vec::new();
        let mut close = None;
        for (j, line) in lines.iter().enumerate().skip(i + 1) {
            let t = line.trim_matches(SP);
            let m = t.chars().take_while(|&x| x == ':').count();
            if m < 3 {
                continue;
            }
            if t[m..].trim_matches(SP).is_empty() {
                if stack.last() == Some(&m) {
                    stack.pop();
                } else if stack.is_empty() && m == len {
                    close = Some(j);
                    break;
                }
            } else {
                stack.push(m);
            }
        }
        let end = close.unwrap_or(lines.len());
        if let Some((name, attrs)) = self.container(spec) {
            let content = &lines[i + 1..end];
            let children = self.inside(1, |r| r.content(&name, None, content));
            out.push(element(&name, attrs, children));
        }
        close.map_or(end, |j| j + 1)
    }

    fn heading(&mut self, level: usize, t: &str) -> Node {
        let (t, attrs) = heading_braces(t);
        let level = if level > 4 {
            self.diags.push(Diagnostic::new(
                Code::HeadingLevelReduced,
                format!("h{level} reduced to h4"),
            ));
            4
        } else {
            level
        };
        let children = self.inside(1, |r| r.phrasing(&t));
        element(&format!("h{level}"), attrs, children)
    }

    /// The lines of a blockquote or callout at `lines[i]`, without `>`.
    fn quote_lines(&mut self, lines: &[String], mut i: usize, out: &mut Vec<Node>) -> usize {
        let mut inner = Vec::new();
        while i < lines.len() && lines[i].trim_start_matches(SP).starts_with('>') {
            let l = &lines[i].trim_start_matches(SP)[1..];
            inner.push(l.strip_prefix(' ').unwrap_or(l).to_string());
            i += 1;
        }
        out.push(self.quote(&inner));
        i
    }

    /// §5.4: an element line at `lines[i]` and its indented content.
    fn element_line(
        &mut self,
        lines: &[String],
        i: usize,
        rest: &str,
        sel: &syntax::Selector,
        out: &mut Vec<Node>,
    ) -> usize {
        let mut j = i + 1;
        while j < lines.len() && (is_blank(&lines[j]) || indent_width(&lines[j]) >= 2) {
            j += 1;
        }
        while j > i + 1 && is_blank(&lines[j - 1]) {
            j -= 1;
        }
        let content: Vec<String> = lines[i + 1..j].iter().map(|l| dedent(l, 2)).collect();
        let (name, attrs) = from_selector(sel);
        let bracket: Option<String> = sel
            .content
            .map(|(a, b)| rest.chars().skip(a).take(b - a).collect());
        let children = self.inside(1, |r| r.content(&name, bracket, &content));
        out.push(element(&name, attrs, children));
        j
    }

    /// A comment block at `lines[i]`: its text and the lines it takes, when
    /// it is closed and nothing follows it on its last line.
    fn block_comment(&self, joined: &Joined, lines: &[String], i: usize) -> Option<Found> {
        let from = joined.from(i, &lines[i]);
        let body = joined.text[from..].strip_prefix("<!--")?;
        let k =
            joined.dashes[joined.dashes.partition_point(|&x| x < from + 4)..].first()? - (from + 4);
        let tail = body[k + 3..].split('\n').next().unwrap_or("");
        if !tail.trim_matches(SP).is_empty() {
            return None;
        }
        let inner = body[..k].to_string();
        let used = inner.matches('\n').count() + 1;
        Some(Found::Comment(inner, used))
    }

    /// A typed HTML block at `lines[i]` that is taken (§6): its nodes, the
    /// lines it takes up to its last, and what follows it on that line.
    fn typed_block(&self, joined: &mut Joined, lines: &[String], i: usize) -> Option<Found> {
        let from = joined.from(i, &lines[i]);
        let rest = &joined.text[from..];
        let name: String = rest[1..]
            .chars()
            .take_while(|x| x.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let ends = joined.ends.entry(name).or_default();
        let len = html::fragment_len_in(rest, from, ends)?;
        let fragment = &rest[..len];
        let remainder = rest[len..].split('\n').next().unwrap_or("").to_string();
        let nodes = html::parse(fragment);
        let blocked = self
            .profile
            .diagnose(&nodes, false)
            .iter()
            .any(|d| d.code.is_blocking());
        let nodes: Vec<Node> = nodes.into_iter().filter(|n| !n.is_blank_text()).collect();
        if blocked || nodes.iter().any(|n| matches!(n, Node::Text(_))) {
            // Not taken: literal text as a whole, to its end tag (§6).
            let mut text_lines: Vec<(String, String)> = fragment
                .split('\n')
                .map(|l| (literal(l), l.to_string()))
                .collect();
            if let Some((line, typed)) = text_lines.last_mut() {
                line.push_str(&remainder);
                typed.push_str(&remainder);
            }
            let first = fragment.split('\n').next().unwrap_or("");
            let diag = Diagnostic::new(Code::HtmlNotTaken, first.to_string());
            return Some(Found::Literal(text_lines, diag));
        }
        let used = fragment.matches('\n').count();
        let remainder = (!remainder.trim_matches(SP).is_empty()).then_some(remainder);
        Some(Found::Typed(nodes, used, remainder))
    }

    fn flush(
        &mut self,
        para: &mut Para,
        out: &mut Vec<Node>,
        pending: &mut Option<Vec<(String, String)>>,
    ) {
        if para.is_empty() {
            return;
        }
        let para = std::mem::take(para);
        let top = self.depth == 0;
        if top {
            self.too_deep = false;
        }
        let mut p = self.paragraph(&para.lines);
        // A top-level paragraph that would be too deep stays text (§3.4).
        if top && (self.too_deep || tree_depth(std::slice::from_ref(&p)) > html::MAX_DEPTH) {
            self.diags.push(html::too_deep());
            let lines: Vec<String> = para.typed.iter().map(|l| literal(l)).collect();
            p = self.paragraph(&lines);
        }
        if let (Some(attrs), Node::Element(e)) = (pending.take(), &mut p) {
            self.merge(e, attrs);
        }
        out.push(p);
    }

    /// Attributes from braces merged into an element (§6): classes united,
    /// braces first; on a conflict the element line wins.
    fn merge(&mut self, e: &mut Element, braces: Vec<(String, String)>) {
        for (name, value) in braces {
            match e.attrs.iter_mut().find(|a| a.name == name) {
                Some(a) if name == "class" => {
                    a.value = format!("{value} {}", a.value);
                }
                Some(a) if a.value != value => {
                    self.diags.push(Diagnostic::new(
                        Code::AttributeConflict,
                        format!(
                            "{name}: \"{value}\" in braces, \"{}\" on the element",
                            a.value
                        ),
                    ));
                }
                Some(_) => {}
                None => e.attrs.push(Attr { name, value }),
            }
        }
    }

    /// The element of a fenced container's name: a block element of the
    /// profile, or else a class of a `div`, as Pandoc reads it.
    fn container(&self, spec: &str) -> Option<(String, Vec<Attr>)> {
        if spec.is_empty() {
            return Some(("div".into(), Vec::new()));
        }
        let c: Vec<char> = format!(".{spec}").chars().collect();
        let sel = syntax::selector(&c, 0, c.len())?;
        if sel.end != c.len() || sel.content.is_some() {
            return None;
        }
        let (name, mut attrs) = from_selector(&sel);
        if self.profile.kind(&name) == Some(Kind::Block) {
            return Some((name, attrs));
        }
        let mut classes = vec![name];
        if let Some(i) = attrs.iter().position(|a| a.name == "class") {
            classes.push(attrs.remove(i).value);
        }
        attrs.push(Attr {
            name: "class".into(),
            value: classes.join(" "),
        });
        Some(("div".into(), attrs))
    }

    /// The children of an element by its content model (§5.4.2, §5.4.3).
    fn content(&mut self, name: &str, bracket: Option<String>, lines: &[String]) -> Vec<Node> {
        let model = self.profile.model(name).unwrap_or(Model::Blocks);
        match model {
            Model::Void => Vec::new(),
            Model::Phrasing => match bracket {
                Some(b) => self.phrasing(&b),
                // Only the level's indentation is removed; the rest is
                // whitespace C7 collapses, or the text of a comment.
                None => self.phrasing(&lines.join("\n")),
            },
            Model::Text => {
                if lines.is_empty() {
                    Vec::new()
                } else {
                    vec![text(lines.join("\n"))]
                }
            }
            Model::Either => match bracket {
                Some(b) => self.phrasing(&b),
                None => self.blocks(lines, false),
            },
            Model::Lead => {
                let mut children = match bracket {
                    Some(b) => self.phrasing(&b),
                    None => Vec::new(),
                };
                children.extend(self.blocks(lines, false));
                children
            }
            _ => self.blocks(lines, name == "figure"),
        }
    }

    /// §5.7 and §7.3.
    fn quote(&mut self, inner: &[String]) -> Node {
        if let Some(first) = inner.first()
            && let Some((t, fold, title)) = callout_line(first)
        {
            return self.inside(1, |r| r.callout(&t, fold, title, &inner[1..]));
        }
        let children = self.inside(1, |r| r.blocks(inner, false));
        element("blockquote", Vec::new(), children)
    }

    fn callout(
        &mut self,
        t: &str,
        fold: Option<char>,
        title: Option<String>,
        rest: &[String],
    ) -> Node {
        let title = title
            .filter(|s| !s.trim_matches(SP).is_empty())
            .map(|title| self.inside(1, |r| r.phrasing(&title)));
        let content = self.blocks(rest, false);
        let (node, diag) = self.profile.callout(t, fold, title, content);
        self.diags.extend(diag);
        node
    }

    /// §5.6: a list at `lines[i]`.
    fn list(&mut self, lines: &[String], start: usize, ordered: bool) -> (Node, usize) {
        // Levels count from the list's own first line.
        let base = indent_width(&lines[start]);
        let item_at = |line: &str| -> Option<(Option<u64>, String)> {
            if indent_width(line) >= base + 2 {
                return None;
            }
            match opener(self.profile, line.trim_start_matches(SP), false) {
                Some(Opener::Item { number, content }) if number.is_some() == ordered => {
                    Some((number, content))
                }
                _ => None,
            }
        };
        let mut items: Vec<Vec<String>> = Vec::new();
        let mut first_number = None;
        let mut i = start;
        while let Some((number, content)) = lines.get(i).and_then(|l| item_at(l)) {
            if first_number.is_none() {
                first_number = number;
            }
            let mut body = vec![content];
            i += 1;
            while i < lines.len() {
                if is_blank(&lines[i]) {
                    let next = (i..lines.len()).find(|&k| !is_blank(&lines[k]));
                    match next {
                        Some(k) if indent_width(&lines[k]) >= base + 2 => {
                            body.extend(std::iter::repeat_n(String::new(), k - i));
                            i = k;
                        }
                        Some(k) if item_at(&lines[k]).is_some() => {
                            i = k;
                            break;
                        }
                        _ => break,
                    }
                } else if indent_width(&lines[i]) >= base + 2 {
                    body.push(dedent(&lines[i], base + 2));
                    i += 1;
                } else {
                    break;
                }
            }
            items.push(body);
        }
        let mut children = Vec::new();
        for body in items {
            let lead = !body[0].trim_matches(SP).is_empty()
                && opener(self.profile, body[0].trim_start_matches(SP), false).is_none();
            // ul or ol, li
            let mut nodes = self.inside(2, |r| r.blocks(&body, false));
            let leading_p = matches!(nodes.first(), Some(Node::Element(p)) if p.name == "p" && p.attrs.is_empty());
            if lead
                && leading_p
                && let Node::Element(p) = nodes.remove(0)
            {
                let mut lead_nodes = p.children;
                lead_nodes.extend(nodes);
                nodes = lead_nodes;
            }
            children.push(element("li", Vec::new(), nodes));
        }
        let mut attrs = Vec::new();
        if ordered && let Some(n) = first_number.filter(|&n| n != 1) {
            attrs.push(Attr {
                name: "start".into(),
                value: n.to_string(),
            });
        }
        // Blank lines after the last item belong to the context.
        while i > start && is_blank(&lines[i - 1]) {
            i -= 1;
        }
        (
            element(if ordered { "ol" } else { "ul" }, attrs, children),
            i,
        )
    }
}

/// `[!type]`, `[!type]-`, `[!type]+`, then optionally a space and the title.
pub(crate) fn callout_line(line: &str) -> Option<(String, Option<char>, Option<String>)> {
    let rest = line.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let t = &rest[..close];
    let mut chars = t.chars();
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
    let title = match after {
        "" => None,
        s if s.starts_with(' ') => Some(s[1..].to_string()),
        _ => return None,
    };
    Some((t.to_ascii_lowercase(), fold, title))
}

/// `Titel {#anker .klasse}` at the end of a heading (§6).
fn heading_braces(t: &str) -> (String, Vec<Attr>) {
    if t.ends_with('}')
        && let Some(open) = t.rfind(" {")
        && let Some(attrs) = syntax::brace_attributes(&t[open + 1..])
    {
        let attrs = attrs
            .into_iter()
            .map(|(name, value)| Attr { name, value })
            .collect();
        return (t[..open].trim_end_matches(SP).to_string(), attrs);
    }
    (t.to_string(), Vec::new())
}

/// W2 at the start of a line: if the line would open a block, the index of
/// the character to put a backslash before.
pub(crate) fn opener_escape(profile: &Profile, line: &[char]) -> Option<usize> {
    let s: String = line.iter().collect();
    opener(profile, &s, false)?;
    line.iter().position(|c| c.is_ascii_punctuation())
}
