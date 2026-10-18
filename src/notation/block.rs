//! Reading notation: the line model (SPEC §5.2) and every block construct
//! of §5 and §6.

use super::inline::{self, Inline, element, from_selector, text};
use super::syntax;
use super::table;
use super::track::{Mark, Src, Track};
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Element, Node};
use crate::html;
use crate::profile::{Kind, Model, Profile};

pub(crate) struct Reader<'p> {
    pub profile: &'p Profile,
    pub diags: Vec<Diagnostic>,
    /// What is recorded for an editor's spans, when it is asked for.
    pub track: Option<Track>,
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
struct Para<'a> {
    lines: Vec<Src>,
    typed: Vec<&'a str>,
}

impl<'a> Para<'a> {
    /// A line that reads as it was typed.
    fn push(&mut self, line: &'a str, track: Option<&Track>) {
        self.lines.push(Src::of(line, track));
        self.typed.push(line);
    }

    /// A line of literal text: `line` reads as `typed`.
    fn push_literal(&mut self, line: Src, typed: &'a str) {
        self.lines.push(line);
        self.typed.push(typed);
    }

    fn pop(&mut self) -> Option<(Src, &'a str)> {
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
    /// Byte offset of each line, and where it begins in the notation.
    starts: Vec<usize>,
    sources: Vec<Option<usize>>,
    /// Where `-->` begins, in order.
    dashes: Vec<usize>,
    /// Ends of typed fragments per tag name (`html::fragment_len_in`).
    ends: std::collections::HashMap<String, std::collections::HashMap<usize, Option<usize>>>,
}

impl Joined {
    fn new(lines: &[&str], track: Option<&Track>) -> Self {
        let text = lines.join("\n");
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for line in lines {
            starts.push(at);
            at += line.len() + 1;
        }
        let sources = lines
            .iter()
            .map(|l| track.and_then(|t| t.offset(l)))
            .collect();
        let dashes = text.match_indices("-->").map(|(k, _)| k).collect();
        Joined {
            text,
            starts,
            sources,
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

    /// Where the byte `at` of the text is in the notation.
    fn source(&self, at: usize) -> Option<usize> {
        let line = self.starts.partition_point(|&s| s <= at).checked_sub(1)?;
        Some(self.sources[line]? + at - self.starts[line])
    }
}

/// A comment or typed HTML block found at a line, before it is taken.
enum Found<'a> {
    Comment(String, usize),
    Typed(Vec<Node>, usize, Option<&'a str>),
    /// A typed block that is closed but not taken (§6): its lines, as the
    /// inline reader reads them (nothing in them is notation, then what
    /// follows its end tag) and as typed; and the diagnostic.
    Literal(Vec<(Src, &'a str)>, Diagnostic),
}

/// Lines `from..to` of a context without their indentation.
fn input_lines<'a>(lines: &[&'a str], from: usize, to: usize) -> Vec<&'a str> {
    lines[from..to.min(lines.len())]
        .iter()
        .map(|l| l.trim_start_matches(SP))
        .collect()
}

/// What a line opens (§5.2), seen without its indentation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Opener<'a> {
    Fence {
        ch: char,
        len: usize,
        info: &'a str,
    },
    Comment,
    TypedBlock,
    Container {
        len: usize,
        spec: &'a str,
    },
    AttrLine(Vec<(String, String)>),
    Heading {
        level: usize,
        text: &'a str,
    },
    Hr,
    Quote,
    Grid,
    CellTable,
    Item {
        number: Option<u64>,
        content: &'a str,
    },
    /// `: ` or `:` alone: a definition, if a one-line paragraph is above it
    /// (§5.6.1).
    Definition(&'a str),
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
pub(crate) fn dedent(line: &str, width: usize) -> &str {
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
    &line[cut..]
}

pub(crate) fn is_blank(line: &str) -> bool {
    line.trim_matches(SP).is_empty()
}

/// What `rest` (a line without indentation) opens; None for a text line.
/// `figure` admits an `img` element line (§5.4.4).
pub(crate) fn opener<'a>(profile: &Profile, rest: &'a str, figure: bool) -> Option<Opener<'a>> {
    let c: Vec<char> = rest.chars().collect();
    let first = *c.first()?;
    let run = |ch: char| c.iter().take_while(|&&x| x == ch).count();
    match first {
        '`' | '~' if run(first) >= 3 => {
            let len = run(first);
            let info = rest[len..].trim_matches(SP);
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
        ':' if c.len() == 1 || c.get(1) == Some(&' ') => {
            Some(Opener::Definition(rest.get(2..).unwrap_or("")))
        }
        ':' if run(':') >= 3 => {
            let len = run(':');
            Some(Opener::Container {
                len,
                spec: rest[len..].trim_matches(SP),
            })
        }
        '{' => syntax::brace_attributes(rest).map(Opener::AttrLine),
        '#' => {
            let level = run('#');
            match c.get(level) {
                Some(' ') => Some(Opener::Heading {
                    level,
                    text: rest[level + 1..].trim_matches(SP),
                }),
                _ => None,
            }
        }
        '-' if rest.trim_end_matches(SP) == "---" => Some(Opener::Hr),
        '-' if c.len() == 1 || c.get(1) == Some(&' ') => Some(Opener::Item {
            number: None,
            content: rest.get(2..).unwrap_or(""),
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
                        content: rest.get(digits + 2..).unwrap_or(""),
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
            track: None,
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

    /// `s`, a slice of the notation, as phrasing to read.
    pub(crate) fn src(&self, s: &str) -> Src {
        Src::of(s, self.track.as_ref())
    }

    /// Lines, slices of the notation, joined as phrasing to read.
    fn joined(&self, lines: &[&str]) -> Src {
        let track = self.track.as_ref();
        let parts = lines
            .iter()
            .map(|l| (self.src(l), track.and_then(|t| t.line_end(l))))
            .collect();
        Src::join(parts, track)
    }

    /// How many records there are, to go back to when a reading is undone.
    fn checkpoint(&self) -> usize {
        self.track.as_ref().map_or(0, |t| t.marks.len())
    }

    fn undo(&mut self, checkpoint: usize) {
        if let Some(t) = &mut self.track {
            t.marks.truncate(checkpoint);
        }
    }

    /// Records lines that are text as they are (code), with the line feeds
    /// between them.
    fn mark_text_lines(&mut self, lines: &[&str]) {
        let Some(t) = &mut self.track else {
            return;
        };
        for (k, line) in lines.iter().enumerate() {
            t.mark_slice(line, Mark::Content);
            if k + 1 < lines.len()
                && let Some(lf) = t.line_end(line)
            {
                t.mark(lf, lf + 1, Mark::Content);
            }
        }
    }

    /// Records the text of parsed HTML whose byte `b` is at `source(b)` in
    /// the notation.
    fn mark_html(
        &mut self,
        nodes: &[Node],
        fragment: &str,
        source: &dyn Fn(usize) -> Option<usize>,
    ) {
        if self.track.is_none() {
            return;
        }
        for node in nodes {
            match node {
                Node::Text(t) => {
                    let Some(text) = fragment.get(t.span.clone()) else {
                        continue;
                    };
                    for (b, ch) in text.char_indices() {
                        if let (Some(o), Some(track)) = (source(t.span.start + b), &mut self.track)
                        {
                            track.mark(o, o + ch.len_utf8(), Mark::Content);
                        }
                    }
                }
                Node::Element(e) => self.mark_html(&e.children, fragment, source),
                _ => {}
            }
        }
    }

    /// The phrasing content of an element open at the current depth.
    pub(crate) fn phrasing(&mut self, s: &Src) -> Vec<Node> {
        let mut r = Inline::within(self.profile, self.depth, self.too_deep);
        if self.track.is_some() {
            r.at = s.at.clone();
        }
        let nodes = inline::unwrap_fragments(r.read(&s.text));
        self.too_deep = r.too_deep;
        self.diags.append(&mut r.diags);
        if let Some(t) = &mut self.track {
            for (from, to, mark) in r.marks {
                t.mark(from, to, mark);
            }
        }
        nodes
    }

    fn paragraph(&mut self, lines: &[Src], typed: &[&str]) -> Node {
        let track = self.track.as_ref();
        let parts = lines
            .iter()
            .zip(typed)
            .map(|(l, t)| (l.clone(), track.and_then(|tr| tr.line_end(t))))
            .collect();
        let text = Src::join(parts, track);
        let children = self.inside(1, |r| r.phrasing(&text));
        element("p", Vec::new(), children)
    }

    /// Reads the lines of one context, already without its indentation.
    pub(crate) fn blocks<'a>(&mut self, input: &[&'a str], figure: bool) -> Vec<Node> {
        let mut lines: Vec<&'a str> = input.to_vec();
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
            if is_blank(lines[i]) {
                self.flush(&mut para, &mut out, &mut pending);
                i += 1;
                continue;
            }
            let rest: &'a str = lines[i].trim_start_matches(SP);
            let op = (!capped)
                .then(|| opener(self.profile, rest, figure))
                .flatten();
            let Some(op) = op else {
                para.push(rest, self.track.as_ref());
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
                para.push(rest, self.track.as_ref());
                i += 1;
                continue;
            }
            // A definition needs a one-line paragraph above it, its term.
            if matches!(op, Opener::Definition(_)) && para.len() != 1 {
                para.push(rest, self.track.as_ref());
                i += 1;
                continue;
            }
            // What only looks like a comment or typed HTML block is text.
            let found = match &op {
                Opener::Comment | Opener::TypedBlock => {
                    let joined =
                        joined.get_or_insert_with(|| Joined::new(input, self.track.as_ref()));
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
                    if is_blank(typed) {
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
                para.push(rest, self.track.as_ref());
                i += 1;
                continue;
            }
            let takes_line = matches!(op, Opener::Grid | Opener::CellTable | Opener::Definition(_));
            let caption = if takes_line && para.len() == 1 {
                para.pop()
            } else {
                self.flush(&mut para, &mut out, &mut pending);
                None
            };
            let caption_typed = caption.as_ref().map(|&(_, typed)| typed);
            let caption = caption.map(|(line, _)| line);
            let before = out.len();
            let line_before = i;
            let line_was = lines[i];
            let marks_before = self.checkpoint();
            let top = self.depth == 0;
            if top {
                self.too_deep = false;
            }
            i = match found {
                Some(found) => self.take_found(found, &mut lines, i, &mut out),
                None => self.take(op, &mut lines, i, rest, caption, &mut out),
            };
            // A top-level block that would be too deep stays text as a
            // whole (§3.4): its lines, as typed, are text lines.
            if top && (self.too_deep || tree_depth(&out[before..]) > html::MAX_DEPTH) {
                out.truncate(before);
                self.undo(marks_before);
                self.diags.push(html::too_deep());
                let typed = caption_typed
                    .into_iter()
                    .chain(input_lines(&lines, line_before, i));
                for line in typed {
                    if is_blank(line) {
                        self.flush(&mut para, &mut out, &mut pending);
                    } else {
                        para.push_literal(Src::literal(line, self.track.as_ref()), line);
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
            // its line as text. (A typed block on one line with text after
            // it took the line up to that text.)
            if i == line_before && lines[i].len() == line_was.len() {
                out.truncate(before);
                self.undo(marks_before);
                para.push(lines[i].trim_start_matches(SP), self.track.as_ref());
                i += 1;
            }
        }
        self.flush(&mut para, &mut out, &mut pending);
        out
    }

    /// Reads the construct `op` opens at `lines[i]` into `out`; returns the
    /// index of the next line. Every construct that nests has a method of its
    /// own, so that a level of nesting costs little stack.
    fn take<'a>(
        &mut self,
        op: Opener<'a>,
        lines: &mut [&'a str],
        i: usize,
        rest: &'a str,
        caption: Option<Src>,
        out: &mut Vec<Node>,
    ) -> usize {
        match op {
            Opener::Fence { ch, len, info } => self.fence(lines, i, ch, len, info, out),
            // Found before, and taken by `take_found`.
            Opener::Comment | Opener::TypedBlock => i,
            Opener::Container { len, spec } => self.fenced_container(lines, i, len, spec, out),
            // Taken before; a line that only looks like one stays text.
            Opener::AttrLine(_) => i,
            Opener::Heading { level, text: t } => {
                out.push(self.heading(level, t));
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
            Opener::Definition(_) => {
                let term = caption.unwrap_or_default();
                let (node, next) = self.definition_list(lines, i, term);
                out.push(node);
                next
            }
            Opener::ElementLine(sel) => self.element_line(lines, i, rest, &sel, out),
        }
    }

    /// Takes a comment or typed HTML block found at `lines[i]`.
    fn take_found<'a>(
        &mut self,
        found: Found<'a>,
        lines: &mut [&'a str],
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
        lines: &[&str],
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
            let nodes: Vec<Node> = nodes.into_iter().filter(|n| !n.is_blank_text()).collect();
            let src = self.joined(&lines[i + 1..end]);
            let starts: Vec<usize> = src.text.char_indices().map(|(b, _)| b).collect();
            let source = |b: usize| {
                let k = starts.partition_point(|&s| s < b);
                (starts.get(k) == Some(&b))
                    .then(|| src.at.get(k).copied().flatten())
                    .flatten()
            };
            self.mark_html(&nodes, &content, &source);
            out.extend(nodes);
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
                self.mark_text_lines(&lines[i + 1..end]);
                vec![text(content)]
            };
            out.push(element("pre", attrs, children));
        }
        close.map_or(end, |j| j + 1)
    }

    /// A fenced container (§5.4, §7.3) at `lines[i]`.
    fn fenced_container(
        &mut self,
        lines: &[&str],
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
        let t = self.src(t);
        let children = self.inside(1, |r| r.phrasing(&t));
        element(&format!("h{level}"), attrs, children)
    }

    /// The lines of a blockquote or callout at `lines[i]`, without `>`.
    fn quote_lines(&mut self, lines: &[&str], mut i: usize, out: &mut Vec<Node>) -> usize {
        let mut inner = Vec::new();
        while i < lines.len() && lines[i].trim_start_matches(SP).starts_with('>') {
            let l = &lines[i].trim_start_matches(SP)[1..];
            inner.push(l.strip_prefix(' ').unwrap_or(l));
            i += 1;
        }
        out.push(self.quote(&inner));
        i
    }

    /// §5.4: an element line at `lines[i]` and its indented content.
    fn element_line(
        &mut self,
        lines: &[&str],
        i: usize,
        rest: &str,
        sel: &syntax::Selector,
        out: &mut Vec<Node>,
    ) -> usize {
        let mut j = i + 1;
        while j < lines.len() && (is_blank(lines[j]) || indent_width(lines[j]) >= 2) {
            j += 1;
        }
        while j > i + 1 && is_blank(lines[j - 1]) {
            j -= 1;
        }
        let content: Vec<&str> = lines[i + 1..j].iter().map(|l| dedent(l, 2)).collect();
        let (name, attrs) = from_selector(sel);
        let bracket = sel.content.map(|(a, b)| char_slice(rest, a, b));
        let children = self.inside(1, |r| r.content(&name, bracket, &content));
        out.push(element(&name, attrs, children));
        j
    }

    /// A comment block at `lines[i]`: its text and the lines it takes, when
    /// it is closed and nothing follows it on its last line.
    fn block_comment<'a>(&self, joined: &Joined, lines: &[&'a str], i: usize) -> Option<Found<'a>> {
        let from = joined.from(i, lines[i]);
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
    fn typed_block<'a>(
        &mut self,
        joined: &mut Joined,
        lines: &[&'a str],
        i: usize,
    ) -> Option<Found<'a>> {
        let from = joined.from(i, lines[i]);
        let rest = &joined.text[from..];
        let name: String = rest[1..]
            .chars()
            .take_while(|x| x.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let ends = joined.ends.entry(name).or_default();
        let len = html::fragment_len_in(rest, from, ends)?;
        let fragment = &rest[..len];
        let rest_len = rest[len..].split('\n').next().unwrap_or("").len();
        let used = fragment.matches('\n').count();
        // The lines the fragment stands on, as typed: the last one with what
        // follows the fragment.
        let typed: Vec<&'a str> = (0..=used)
            .map(|k| match k {
                0 => lines[i].trim_start_matches(SP),
                k => lines[i + k],
            })
            .collect();
        let last = typed[used];
        let remainder = &last[last.len() - rest_len..];
        let nodes = html::parse(fragment);
        let blocked = self
            .profile
            .diagnose(&nodes, false)
            .iter()
            .any(|d| d.code.is_blocking());
        let nodes: Vec<Node> = nodes.into_iter().filter(|n| !n.is_blank_text()).collect();
        if blocked || nodes.iter().any(|n| matches!(n, Node::Text(_))) {
            // Not taken: literal text as a whole, to its end tag (§6).
            let track = self.track.as_ref();
            let text_lines = typed
                .iter()
                .enumerate()
                .map(|(k, &t)| match k == used {
                    true => {
                        let cut = t.len() - rest_len;
                        let mut line = Src::literal(&t[..cut], track);
                        line.push(Src::of(&t[cut..], track));
                        (line, t)
                    }
                    false => (Src::literal(t, track), t),
                })
                .collect();
            let first = fragment.split('\n').next().unwrap_or("");
            let diag = Diagnostic::new(Code::HtmlNotTaken, first.to_string());
            return Some(Found::Literal(text_lines, diag));
        }
        self.mark_html(&nodes, fragment, &|b| joined.source(from + b));
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
        let marks_before = self.checkpoint();
        let mut p = self.paragraph(&para.lines, &para.typed);
        // A top-level paragraph that would be too deep stays text (§3.4).
        if top && (self.too_deep || tree_depth(std::slice::from_ref(&p)) > html::MAX_DEPTH) {
            self.undo(marks_before);
            self.diags.push(html::too_deep());
            let track = self.track.as_ref();
            let lines: Vec<Src> = para.typed.iter().map(|l| Src::literal(l, track)).collect();
            p = self.paragraph(&lines, &para.typed);
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
    fn content(&mut self, name: &str, bracket: Option<&str>, lines: &[&str]) -> Vec<Node> {
        let model = self.profile.model(name).unwrap_or(Model::Blocks);
        let bracket = bracket.map(|b| self.src(b));
        match model {
            Model::Void => Vec::new(),
            Model::Phrasing => match bracket {
                Some(b) => self.phrasing(&b),
                // Only the level's indentation is removed; the rest is
                // whitespace C7 collapses, or the text of a comment.
                None => {
                    let text = self.joined(lines);
                    self.phrasing(&text)
                }
            },
            Model::Text => {
                if lines.is_empty() {
                    Vec::new()
                } else {
                    self.mark_text_lines(lines);
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
    fn quote(&mut self, inner: &[&str]) -> Node {
        if let Some(first) = inner.first()
            && let Some((t, fold, title)) = callout_line(first)
        {
            return self.inside(1, |r| r.callout(&t, fold, title, &inner[1..]));
        }
        let children = self.inside(1, |r| r.blocks(inner, false));
        element("blockquote", Vec::new(), children)
    }

    fn callout(&mut self, t: &str, fold: Option<char>, title: Option<&str>, rest: &[&str]) -> Node {
        let title = title
            .filter(|s| !s.trim_matches(SP).is_empty())
            .map(|title| {
                let title = self.src(title);
                self.inside(1, |r| r.phrasing(&title))
            });
        let content = self.blocks(rest, false);
        let (node, diag) = self.profile.callout(t, fold, title, content);
        self.diags.extend(diag);
        node
    }

    /// The content of a list item or definition (content model *lead*):
    /// its lines read as blocks, a leading paragraph unwrapped when the first
    /// line opens no block.
    fn item(&mut self, body: &[&str]) -> Vec<Node> {
        let first = body[0].trim_start_matches(SP);
        let lead = !first.trim_matches(SP).is_empty()
            && matches!(
                opener(self.profile, first, false),
                None | Some(Opener::Definition(_))
            );
        let mut nodes = self.blocks(body, false);
        let leading_p =
            matches!(nodes.first(), Some(Node::Element(p)) if p.name == "p" && p.attrs.is_empty());
        if lead
            && leading_p
            && let Node::Element(p) = nodes.remove(0)
        {
            let mut lead_nodes = p.children;
            lead_nodes.extend(nodes);
            nodes = lead_nodes;
        }
        nodes
    }

    /// §5.6.1: a definition list whose first definition is at `lines[start]`,
    /// below `term`.
    fn definition_list(&mut self, lines: &[&str], start: usize, term: Src) -> (Node, usize) {
        let base = indent_width(lines[start]);
        let at_level = |line: &str| indent_width(line) < base + 2;
        let profile = self.profile;
        let def_at = |line: &str| -> bool {
            at_level(line)
                && matches!(
                    opener(profile, line.trim_start_matches(SP), false),
                    Some(Opener::Definition(_))
                )
        };
        // A term: a text line directly followed by a definition.
        let term_at = |k: usize| -> bool {
            let line = lines[k];
            !is_blank(line)
                && at_level(line)
                && opener(profile, line.trim_start_matches(SP), false).is_none()
                && lines.get(k + 1).is_some_and(|l| def_at(l))
        };
        let mut children = Vec::new();
        let mut term = Some(term);
        let mut i = start;
        loop {
            if let Some(t) = term.take() {
                // dl, dt
                let nodes = self.inside(2, |r| r.phrasing(&t.trim()));
                children.push(element("dt", Vec::new(), nodes));
            }
            while let Some(line) = lines.get(i).copied().filter(|l| def_at(l)) {
                let Some(Opener::Definition(content)) =
                    opener(profile, line.trim_start_matches(SP), false)
                else {
                    break;
                };
                let next_item = |l: &str| def_at(l) || !is_blank(l) && at_level(l);
                let (body, next) = item_body(lines, i, base, content, next_item);
                // dl, dd
                let nodes = self.inside(2, |r| r.item(&body));
                children.push(element("dd", Vec::new(), nodes));
                i = next;
            }
            let Some(k) = (i..lines.len()).find(|&k| !is_blank(lines[k])) else {
                break;
            };
            if !term_at(k) {
                break;
            }
            term = Some(self.src(lines[k].trim_start_matches(SP)));
            i = k + 1;
        }
        // Blank lines after the last definition belong to the context.
        while i > start && is_blank(lines[i - 1]) {
            i -= 1;
        }
        (element("dl", Vec::new(), children), i)
    }

    /// §5.6: a list at `lines[i]`.
    fn list(&mut self, lines: &[&str], start: usize, ordered: bool) -> (Node, usize) {
        // Levels count from the list's own first line.
        let base = indent_width(lines[start]);
        let profile = self.profile;
        let item_at = |line: &str| -> Option<Option<u64>> {
            if indent_width(line) >= base + 2 {
                return None;
            }
            match opener(profile, line.trim_start_matches(SP), false) {
                Some(Opener::Item { number, .. }) if number.is_some() == ordered => Some(number),
                _ => None,
            }
        };
        let mut items: Vec<Vec<&str>> = Vec::new();
        let mut first_number = None;
        let mut i = start;
        while let Some(line) = lines.get(i).copied() {
            let Some(Opener::Item { number, content }) =
                item_at(line).and_then(|_| opener(profile, line.trim_start_matches(SP), false))
            else {
                break;
            };
            if first_number.is_none() {
                first_number = number;
            }
            let (body, next) = item_body(lines, i, base, content, |l| item_at(l).is_some());
            items.push(body);
            i = next;
        }
        let mut children = Vec::new();
        for body in items {
            // ul or ol, li
            let nodes = self.inside(2, |r| r.item(&body));
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
        while i > start && is_blank(lines[i - 1]) {
            i -= 1;
        }
        (
            element(if ordered { "ol" } else { "ul" }, attrs, children),
            i,
        )
    }
}

/// The characters `a..b` of `s`, by char index.
fn char_slice(s: &str, a: usize, b: usize) -> &str {
    let byte = |k: usize| s.char_indices().nth(k).map_or(s.len(), |(i, _)| i);
    &s[byte(a)..byte(b)]
}

/// `[!type]`, `[!type]-`, `[!type]+`, then optionally a space and the title.
pub(crate) fn callout_line(line: &str) -> Option<(String, Option<char>, Option<&str>)> {
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
        s if s.starts_with(' ') => Some(&s[1..]),
        _ => return None,
    };
    Some((t.to_ascii_lowercase(), fold, title))
}

/// `Titel {#anker .klasse}` at the end of a heading (§6).
fn heading_braces(t: &str) -> (&str, Vec<Attr>) {
    if t.ends_with('}')
        && let Some(open) = t.rfind(" {")
        && let Some(attrs) = syntax::brace_attributes(&t[open + 1..])
    {
        let attrs = attrs
            .into_iter()
            .map(|(name, value)| Attr { name, value })
            .collect();
        return (t[..open].trim_end_matches(SP), attrs);
    }
    (t, Vec::new())
}

/// The lines of an item whose first line, at `lines[i]`, holds `content`:
/// lines indented below the item's level, and blank lines followed by such
/// lines; blank lines followed by the next item (`next`) end it. Returns the
/// body and the index after it.
fn item_body<'a>(
    lines: &[&'a str],
    mut i: usize,
    base: usize,
    content: &'a str,
    next: impl Fn(&str) -> bool,
) -> (Vec<&'a str>, usize) {
    let mut body = vec![content];
    i += 1;
    while i < lines.len() {
        if is_blank(lines[i]) {
            match (i..lines.len()).find(|&k| !is_blank(lines[k])) {
                Some(k) if indent_width(lines[k]) >= base + 2 => {
                    // Blank lines inside an item are empty.
                    body.extend(lines[i..k].iter().map(|l| &l[l.len()..]));
                    i = k;
                }
                Some(k) if next(lines[k]) => {
                    i = k;
                    break;
                }
                _ => break,
            }
        } else if indent_width(lines[i]) >= base + 2 {
            body.push(dedent(lines[i], base + 2));
            i += 1;
        } else {
            break;
        }
    }
    (body, i)
}

/// W2 at the start of a line: if the line would open a block, the index of
/// the character to put a backslash before.
/// `line` is the line's index in its run of phrasing: a definition opens
/// only directly after a first line (§5.6.1).
pub(crate) fn opener_escape(profile: &Profile, line: &[char], index: usize) -> Option<usize> {
    let s: String = line.iter().collect();
    if let Opener::Definition(_) = opener(profile, &s, false)?
        && index != 1
    {
        return None;
    }
    line.iter().position(|c| c.is_ascii_punctuation())
}
