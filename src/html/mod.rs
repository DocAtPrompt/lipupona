//! A parser for the body of an HTML document that keeps source spans.
//!
//! It follows the tokenizer of the HTML Living Standard closely and its tree
//! construction as far as body content needs it: optional end tags, implied
//! table sections and rows, void elements, the line feed after `<pre>`,
//! raw text elements and self-closing tags in SVG and MathML. The head is
//! collected apart from the body ([`parse_document`]); `html`, `head` and
//! `body` tags give their attributes, the doctype is skipped. What it
//! does not do — foster parenting, the adoption agency for misnested
//! formatting elements — concerns HTML that is outside every profile anyway
//! and is passed through as it is.

mod entities;

use crate::dom::{Attr, Comment, Element, Node, Text};

/// Nesting deeper than this is not built; an element named [`TOO_DEEP`]
/// marks the place, so that the block is passed through as stored. It keeps
/// every walk over the tree within a fixed depth.
pub const MAX_DEPTH: usize = 256;

/// The name of the element that marks nesting beyond [`MAX_DEPTH`].
pub const TOO_DEEP: &str = "#too-deep";

/// The report for nesting beyond [`MAX_DEPTH`], in HTML or in notation.
pub(crate) fn too_deep() -> crate::Diagnostic {
    crate::Diagnostic::new(
        crate::Code::LimitExceeded,
        format!("nesting deeper than {MAX_DEPTH} elements"),
    )
}

/// Parses the body of an HTML document and returns the children of `body`.
/// Spans are byte offsets into `input`.
pub fn parse(input: &str) -> Vec<Node> {
    parse_with(input, false).nodes
}

/// A whole document: the children of `body`, and what stands around them.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub nodes: Vec<Node>,
    /// The children of the head in order, then comments outside `html`.
    pub head: Vec<HeadPart>,
    pub html_attrs: Vec<Attr>,
    pub head_attrs: Vec<Attr>,
    pub body_attrs: Vec<Attr>,
}

/// A child of the head: its source (LF line ends), and for an element its
/// name, its attributes and, for `title`, its decoded text, for `style` its
/// CSS as it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadPart {
    pub source: String,
    pub element: Option<(String, Vec<Attr>)>,
    pub text: String,
}

/// Parses a whole document (SPEC §3.7): the body as [`parse`] does, and the
/// head, as a browser ends it — at `</head>`, at `<body>`, or at the first
/// content that has no place in a head.
pub fn parse_document(input: &str) -> Document {
    parse_with(input, true)
}

fn parse_with(input: &str, document: bool) -> Document {
    let prepared = prepare(input);
    // A document's body ends at its last `</body>` when only comments and
    // `</html>` follow, so that an element left open in it cannot take the
    // end of the document in (C14).
    let (src, tail) = match document {
        true => split_tail(&prepared.text),
        false => (prepared.text.as_str(), ""),
    };
    let mut p = Parser {
        src,
        pos: 0,
        stack: Vec::new(),
        root: Vec::new(),
        in_head: false,
        ignore_lf: false,
        too_deep: false,
        skipped: Vec::new(),
        document,
        outside: document,
        in_body: false,
        doc: Document::default(),
        outside_parts: Vec::new(),
    };
    p.run();
    let mut doc = std::mem::take(&mut p.doc);
    doc.head.append(&mut p.outside_parts);
    let mut rest = tail;
    while let Some(k) = rest.find("<!--") {
        let len = rest[k + 4..].find("-->").map_or(rest.len() - k, |e| e + 7);
        doc.head.push(HeadPart {
            source: rest[k..k + len].to_string(),
            element: None,
            text: String::new(),
        });
        rest = &rest[k + len..];
    }
    doc.nodes = p.root;
    for node in doc.nodes.iter_mut() {
        remap(node, &prepared.removed);
    }
    doc
}

/// `text` up to its last `</body>`, and what follows, when only whitespace,
/// comments and `</html>` follow it; else all of `text`.
fn split_tail(text: &str) -> (&str, &str) {
    let lower = text.to_ascii_lowercase();
    let Some(at) = lower.rfind("</body") else {
        return (text, "");
    };
    let mut rest = &lower[at + 6..];
    let Some(gt) = rest.find('>') else {
        return (text, "");
    };
    if !rest[..gt]
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .is_empty()
    {
        return (text, "");
    }
    rest = &rest[gt + 1..];
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        if rest.is_empty() {
            break;
        }
        if let Some(r) = rest.strip_prefix("<!--") {
            match r.find("-->") {
                Some(e) => rest = &r[e + 3..],
                None => return (text, ""),
            }
        } else if let Some(r) = rest.strip_prefix("</html") {
            match r.find('>') {
                Some(e) if r[..e].trim().is_empty() => rest = &r[e + 1..],
                _ => return (text, ""),
            }
        } else {
            return (text, "");
        }
    }
    (&text[..at], &text[at..])
}

/// Elements that belong in a head; any other start tag ends it.
const HEAD_ELEMENTS: &[&str] = &[
    "base", "basefont", "bgsound", "link", "meta", "title", "noscript", "noframes", "style",
    "script", "template",
];

/// Text with CRLF and CR turned into LF (C2). A U+FEFF stays: inside a
/// document it is a character, not a byte order mark.
pub fn lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// The input as an HTML parser sees it (C1, C2): without a byte order mark,
/// with CRLF and CR turned into LF.
struct Prepared {
    text: String,
    /// Offsets in `text` from which on one more byte of the input lies
    /// behind, in ascending order.
    removed: Vec<usize>,
}

fn prepare(input: &str) -> Prepared {
    let mut removed = Vec::new();
    let body = match input.strip_prefix('\u{feff}') {
        Some(rest) => {
            removed.extend([0, 0, 0]);
            rest
        }
        None => input,
    };
    let mut text = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            text.push('\n');
            if chars.peek() == Some(&'\n') {
                chars.next();
                removed.push(text.len());
            }
        } else {
            text.push(c);
        }
    }
    Prepared { text, removed }
}

fn original(pos: usize, removed: &[usize]) -> usize {
    pos + removed.partition_point(|&r| r <= pos)
}

fn remap(node: &mut Node, removed: &[usize]) {
    if removed.is_empty() {
        return;
    }
    let span = match node {
        Node::Element(e) => {
            for c in e.children.iter_mut() {
                remap(c, removed);
            }
            &mut e.span
        }
        Node::Text(t) => &mut t.span,
        Node::Comment(c) => &mut c.span,
        Node::Verbatim(_) => return,
    };
    *span = original(span.start, removed)..original(span.end, removed);
}

/// Elements without content: the void elements, and the obsolete ones a
/// browser's parser closes at once (`frame`, `param`, …).
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

const RAW_TEXT: &[&str] = &["script", "style", "xmp", "iframe", "noembed", "noframes"];
const RCDATA: &[&str] = &["textarea", "title"];

/// Start tags that close an open `p`.
const CLOSES_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "header",
    "hgroup",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "search",
    "section",
    "summary",
    "ul",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "listing",
    "form",
    "table",
    "hr",
    "li",
    "dd",
    "dt",
    "plaintext",
    "xmp",
];

const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];

/// Elements that end the search of an end tag or of an `li` for its partner.
const SPECIAL: &[&str] = &[
    "address",
    "applet",
    "area",
    "article",
    "aside",
    "base",
    "blockquote",
    "body",
    "br",
    "button",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dir",
    "div",
    "dl",
    "dt",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "html",
    "iframe",
    "img",
    "input",
    "li",
    "listing",
    "main",
    "marquee",
    "menu",
    "nav",
    "object",
    "ol",
    "p",
    "plaintext",
    "pre",
    "search",
    "section",
    "select",
    "summary",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
    "xmp",
];

/// End tags that close their element with everything open inside it, if
/// it is in scope, and are ignored otherwise (WHATWG, "in body").
const BLOCK_END: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "button",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "header",
    "hgroup",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "pre",
    "search",
    "section",
    "summary",
    "ul",
];

const TABLE_PARTS: &[&str] = &[
    "table", "caption", "thead", "tbody", "tfoot", "tr", "td", "th",
];

const SCOPE: &[&str] = &[
    "applet",
    "caption",
    "html",
    "table",
    "td",
    "th",
    "marquee",
    "object",
    "template",
    "foreignobject",
    "desc",
    "title",
];

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    stack: Vec<Element>,
    root: Vec<Node>,
    in_head: bool,
    ignore_lf: bool,
    /// Whether a whole document is read: comments outside `html` then go
    /// with the head.
    document: bool,
    /// Before the `html` start tag, or after its end tag.
    outside: bool,
    /// The body has begun: a `head` start tag is ignored, as in a browser.
    in_body: bool,
    doc: Document,
    outside_parts: Vec<HeadPart>,
    /// The deepest open element holds a [`TOO_DEEP`] mark already.
    too_deep: bool,
    /// Start tags beyond [`MAX_DEPTH`] that were not built, innermost last:
    /// their end tags belong to them, not to the elements that are open.
    skipped: Vec<String>,
}

impl<'a> Parser<'a> {
    fn run(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() {
            if bytes[self.pos] == b'<' && self.markup() {
                continue;
            }
            // Text up to the next `<` that may begin markup.
            let start = self.pos;
            let mut end = start + 1;
            while end < bytes.len() && bytes[end] != b'<' {
                end += 1;
            }
            self.pos = end;
            let text = decode(&self.src[start..end], false);
            self.text(text, start..end);
        }
        let end = self.src.len();
        while !self.stack.is_empty() {
            self.pop(end, true);
        }
    }

    /// Reads markup at `<`. Returns false when the `<` is text.
    fn markup(&mut self) -> bool {
        let rest = &self.src[self.pos..];
        let start = self.pos;
        if let Some(body) = rest.strip_prefix("<!--") {
            let (text, len) = match body.find("-->") {
                Some(i) => (&body[..i], 4 + i + 3),
                None => (body, rest.len()),
            };
            self.pos += len;
            // A comment of the head that is not closed is closed (C17).
            let closed = rest[..len].ends_with("-->") && len >= 7;
            let part = || HeadPart {
                source: match closed {
                    true => rest[..len].to_string(),
                    false => format!("{}-->", &rest[..len]),
                },
                element: None,
                text: String::new(),
            };
            if self.in_head {
                self.doc.head.push(part());
            } else if self.outside {
                self.outside_parts.push(part());
            } else {
                self.ignore_lf = false;
                let node = Node::Comment(Comment {
                    text: text.to_string(),
                    span: start..self.pos,
                });
                self.append(node);
            }
            return true;
        }
        let next = rest.as_bytes().get(1).copied();
        match next {
            Some(b'!') | Some(b'?') => {
                // Doctype or bogus comment: skipped.
                self.pos += rest.find('>').map_or(rest.len(), |i| i + 1);
                true
            }
            Some(b'/') => {
                let after = rest.as_bytes().get(2).copied();
                if !after.is_some_and(|b| b.is_ascii_alphabetic()) {
                    if after == Some(b'>') {
                        self.pos += 3;
                        return true;
                    }
                    return false;
                }
                let Some(tag) = scan_tag(rest, 2) else {
                    return self.unterminated();
                };
                self.pos += tag.len;
                self.end_tag(&tag.name, start, self.pos);
                true
            }
            Some(b) if b.is_ascii_alphabetic() => {
                let Some(tag) = scan_tag(rest, 1) else {
                    return self.unterminated();
                };
                self.pos += tag.len;
                self.start_tag(tag, start);
                true
            }
            _ => false,
        }
    }

    /// A tag that runs to the end of the input. A browser drops it; here the
    /// rest of the input stays text, so that nothing is lost.
    fn unterminated(&mut self) -> bool {
        let start = self.pos;
        self.pos = self.src.len();
        self.text(self.src[start..].to_string(), start..self.pos);
        true
    }

    fn start_tag(&mut self, tag: Tag, start: usize) {
        let end = self.pos;
        let name = tag.name.as_str();
        self.ignore_lf = false;
        match name {
            "html" => {
                self.outside = false;
                if self.doc.html_attrs.is_empty() {
                    self.doc.html_attrs = tag.attrs;
                }
                return;
            }
            "head" if self.in_body => return,
            "head" => {
                self.outside = false;
                self.in_head = true;
                if self.doc.head_attrs.is_empty() {
                    self.doc.head_attrs = tag.attrs;
                }
                return;
            }
            "body" => {
                self.outside = false;
                self.in_head = false;
                self.in_body = true;
                if self.doc.body_attrs.is_empty() {
                    self.doc.body_attrs = tag.attrs;
                }
                return;
            }
            _ => {}
        }
        self.outside = false;
        // Before the body, an element of the head implies the head.
        if self.document && !self.in_body && HEAD_ELEMENTS.contains(&name) {
            self.in_head = true;
        }
        if self.in_head {
            if HEAD_ELEMENTS.contains(&name) {
                self.head_element(tag, start);
                return;
            }
            // Anything else ends the head, as in a browser.
            self.in_head = false;
        }
        self.in_body = true;
        let foreign = self.in_foreign() || name == "svg" || name == "math";
        if !self.in_foreign() {
            self.close_for(name, start);
        }
        let el = Element {
            name: tag.name.clone(),
            attrs: tag.attrs,
            children: Vec::new(),
            span: start..end,
            implied_end: false,
        };
        let void = (foreign && tag.self_closing) || (!self.in_foreign() && VOID.contains(&name));
        if self.stack.len() >= MAX_DEPTH {
            self.mark_too_deep(start..end);
            if void {
                return;
            }
            self.skipped.push(tag.name.clone());
            if RAW_TEXT.contains(&name) || RCDATA.contains(&name) {
                let rest = &self.src[self.pos..];
                self.pos += find_end_tag(rest, name).unwrap_or(rest.len());
            }
            return;
        }
        if void {
            self.append(Node::Element(el));
            return;
        }
        self.stack.push(el);
        if RAW_TEXT.contains(&name) || RCDATA.contains(&name) {
            let text_start = self.pos;
            let close =
                find_end_tag(&self.src[self.pos..], name).map_or(self.src.len(), |i| self.pos + i);
            let raw = &self.src[text_start..close];
            if !raw.is_empty() {
                let text = if RCDATA.contains(&name) {
                    decode(raw, false)
                } else {
                    raw.to_string()
                };
                self.text(text, text_start..close);
            }
            self.pos = close;
        } else if matches!(name, "pre" | "listing" | "textarea") {
            self.ignore_lf = true;
        }
    }

    /// Closes what a start tag closes implicitly and opens the table
    /// sections and rows it implies.
    fn close_for(&mut self, name: &str, at: usize) {
        if CLOSES_P.contains(&name) && self.in_scope("p", &["button"]) {
            self.close_up_to("p", at);
        }
        match name {
            n if HEADINGS.contains(&n) => {
                if self
                    .current()
                    .is_some_and(|e| HEADINGS.contains(&e.name.as_str()))
                {
                    self.pop(at, true);
                }
            }
            "li" => self.close_list_item(&["li"], at),
            "dd" | "dt" => self.close_list_item(&["dd", "dt"], at),
            "caption" | "colgroup" | "thead" | "tbody" | "tfoot" => self.clear_to(&["table"], at),
            "tr" => {
                self.clear_to(&["tbody", "thead", "tfoot", "table"], at);
                if self.current_is("table") {
                    self.implied("tbody", at);
                }
            }
            "td" | "th" => {
                if self.in_scope("td", &[]) || self.in_scope("th", &[]) {
                    while let Some(e) = self.current() {
                        let cell = matches!(e.name.as_str(), "td" | "th");
                        self.pop(at, true);
                        if cell {
                            break;
                        }
                    }
                }
                if self.current_is("table") {
                    self.implied("tbody", at);
                }
                if self
                    .current()
                    .is_some_and(|e| matches!(e.name.as_str(), "tbody" | "thead" | "tfoot"))
                {
                    self.implied("tr", at);
                }
            }
            _ => {}
        }
    }

    fn close_list_item(&mut self, names: &[&str], at: usize) {
        for i in (0..self.stack.len()).rev() {
            let n = self.stack[i].name.as_str();
            if names.contains(&n) {
                while self.stack.len() > i {
                    self.pop(at, true);
                }
                return;
            }
            if SPECIAL.contains(&n) && !matches!(n, "address" | "div" | "p") {
                return;
            }
        }
    }

    /// Pops open table parts until the current element is one of `names`, as
    /// long as a table is open.
    fn clear_to(&mut self, names: &[&str], at: usize) {
        if !self.stack.iter().any(|e| e.name == "table") {
            return;
        }
        while let Some(e) = self.current() {
            if names.contains(&e.name.as_str()) || e.name == "table" || e.name == "template" {
                break;
            }
            self.pop(at, true);
        }
    }

    /// Marks that an element would be deeper than [`MAX_DEPTH`]: once for
    /// each time the open elements reach the cap.
    fn mark_too_deep(&mut self, span: std::ops::Range<usize>) {
        if !std::mem::replace(&mut self.too_deep, true) {
            self.append(Node::Element(Element {
                name: TOO_DEEP.into(),
                attrs: Vec::new(),
                children: Vec::new(),
                span,
                implied_end: false,
            }));
        }
    }

    fn implied(&mut self, name: &str, at: usize) {
        if self.stack.len() >= MAX_DEPTH {
            self.mark_too_deep(at..at);
            return;
        }
        self.stack.push(Element {
            name: name.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            span: at..at,
            implied_end: true,
        });
    }

    fn end_tag(&mut self, name: &str, start: usize, end: usize) {
        self.ignore_lf = false;
        if self.in_head {
            if name == "head" {
                self.in_head = false;
            }
            return;
        }
        if name == "html" && self.document {
            self.outside = true;
            return;
        }
        if !matches!(name, "body" | "head") {
            self.in_body = true;
            self.outside = false;
        }
        if let Some(k) = self.skipped.iter().rposition(|n| n == name) {
            self.skipped.truncate(k);
            return;
        }
        let full = self.stack.len() >= MAX_DEPTH;
        match name {
            "html" | "body" | "head" => {}
            "p" | "br" if full && (name == "br" || !self.in_scope("p", &["button"])) => {
                self.mark_too_deep(start..end);
            }
            "p" if !self.in_scope("p", &["button"]) => {
                // `</p>` without an open p is an empty paragraph.
                self.append(Node::Element(Element {
                    name: "p".into(),
                    attrs: Vec::new(),
                    children: Vec::new(),
                    span: start..end,
                    implied_end: false,
                }));
            }
            "br" => self.append(Node::Element(Element {
                name: "br".into(),
                attrs: Vec::new(),
                children: Vec::new(),
                span: start..end,
                implied_end: false,
            })),
            // A block's end tag closes what is open inside it; without the
            // block in scope it is ignored. `</li>`, `</dd>`, `</dt>` and a
            // heading's end tag likewise.
            _ if !self.in_foreign()
                && (BLOCK_END.contains(&name)
                    || matches!(name, "li" | "dd" | "dt")
                    || HEADINGS.contains(&name)) =>
            {
                let targets: &[&str] = if HEADINGS.contains(&name) {
                    HEADINGS
                } else {
                    std::slice::from_ref(&name)
                };
                let extra: &[&str] = if name == "li" { &["ol", "ul"] } else { &[] };
                if !targets.iter().any(|t| self.in_scope(t, extra)) {
                    return;
                }
                while let Some(e) = self.current() {
                    if targets.contains(&e.name.as_str()) {
                        self.pop(end, false);
                        return;
                    }
                    self.pop(start, true);
                }
            }
            _ => {
                // Elements whose end tags are implied by the end tag of an
                // ancestor; the search goes on past them.
                let table = TABLE_PARTS.contains(&name);
                for i in (0..self.stack.len()).rev() {
                    let n = self.stack[i].name.as_str();
                    if n == name {
                        while self.stack.len() > i + 1 {
                            self.pop(start, true);
                        }
                        self.pop(end, false);
                        return;
                    }
                    let implied = matches!(n, "li" | "dd" | "dt" | "p")
                        || (table && TABLE_PARTS.contains(&n));
                    if SPECIAL.contains(&n) && !implied && !self.in_foreign() {
                        return;
                    }
                }
            }
        }
    }

    fn close_up_to(&mut self, name: &str, at: usize) {
        while let Some(e) = self.current() {
            let found = e.name == name;
            self.pop(at, true);
            if found {
                break;
            }
        }
    }

    fn in_scope(&self, name: &str, extra: &[&str]) -> bool {
        for e in self.stack.iter().rev() {
            if e.name == name {
                return true;
            }
            if SCOPE.contains(&e.name.as_str()) || extra.contains(&e.name.as_str()) {
                return false;
            }
        }
        false
    }

    fn in_foreign(&self) -> bool {
        self.stack
            .iter()
            .any(|e| e.name == "svg" || e.name == "math")
    }

    fn current(&self) -> Option<&Element> {
        self.stack.last()
    }

    fn current_is(&self, name: &str) -> bool {
        self.current().is_some_and(|e| e.name == name)
    }

    fn pop(&mut self, end: usize, implied: bool) {
        let Some(mut el) = self.stack.pop() else {
            return;
        };
        self.too_deep = false;
        self.skipped.clear();
        el.span.end = end.max(el.span.start);
        el.implied_end = implied;
        self.append(Node::Element(el));
    }

    fn append(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => self.root.push(node),
        }
    }

    fn text(&mut self, mut text: String, mut span: std::ops::Range<usize>) {
        let blank = text.chars().all(|c| c.is_ascii_whitespace());
        if self.document && !self.in_body && blank {
            return;
        }
        if self.in_head || self.outside {
            if text.chars().all(|c| c.is_ascii_whitespace()) {
                return;
            }
            // Text ends the head, as in a browser.
            self.in_head = false;
            self.outside = false;
        }
        self.in_body = true;
        if std::mem::take(&mut self.ignore_lf) && text.starts_with('\n') {
            text.remove(0);
            span.start += 1;
        }
        if text.is_empty() {
            return;
        }
        let children = match self.stack.last_mut() {
            Some(parent) => &mut parent.children,
            None => &mut self.root,
        };
        // Text is joined only where nothing was skipped in between, so that
        // a span never covers markup that is not in the tree.
        if let Some(Node::Text(last)) = children
            .last_mut()
            .filter(|n| matches!(n, Node::Text(t) if t.span.end == span.start))
        {
            last.text.push_str(&text);
            last.span.end = span.end;
        } else {
            children.push(Node::Text(Text { text, span }));
        }
    }

    /// An element of the head, from its start tag at `start` (whose end is
    /// `self.pos`) to its end tag: kept whole, with its source.
    fn head_element(&mut self, tag: Tag, start: usize) {
        let name = tag.name.as_str();
        let mut text = String::new();
        let mut unclosed = false;
        if !VOID.contains(&name) && !tag.self_closing {
            let rest = &self.src[self.pos..];
            let found = find_end_tag(rest, name);
            unclosed = found.is_none();
            let close = found.unwrap_or(rest.len());
            match name {
                "title" => text = decode(&rest[..close], false),
                "style" => text = rest[..close].to_string(),
                _ => {}
            }
            let after = rest[close..]
                .find('>')
                .map_or(rest.len(), |k| close + k + 1);
            self.pos += after;
        }
        // An element of the head that is not closed is closed (C17).
        let mut source = self.src[start..self.pos].to_string();
        if unclosed {
            source.push_str(&format!("</{name}>"));
        }
        self.doc.head.push(HeadPart {
            source,
            element: Some((tag.name, tag.attrs)),
            text,
        });
    }
}

/// The length in bytes of an HTML fragment typed into notation (SPEC §6):
/// `s` begins with a start tag, and the fragment runs to its matching end
/// tag, or is the tag alone for a void element. None when the fragment is
/// not closed.
///
/// `s` begins at byte `base` of a longer text. `ends` keeps, for start tags
/// of one name in that text, where their elements end, so that many
/// unclosed tags cost linear time: a nested element is the same search from
/// its own start tag. It must be used for one name and one end of text only.
pub(crate) fn fragment_len_in(
    s: &str,
    base: usize,
    ends: &mut std::collections::HashMap<usize, Option<usize>>,
) -> Option<usize> {
    let tag = scan_tag(s, 1)?;
    if VOID.contains(&tag.name.as_str()) || tag.self_closing {
        return Some(tag.len);
    }
    if let Some(&end) = ends.get(&base) {
        return end.map(|e| e - base);
    }
    let b = s.as_bytes();
    let mut open = vec![0];
    let mut i = tag.len;
    while let Some(off) = s[i..].find('<') {
        let at = i + off;
        let end_tag = b.get(at + 1) == Some(&b'/');
        let from = if end_tag { 2 } else { 1 };
        let named = b[at + from..]
            .get(..tag.name.len())
            .is_some_and(|n| n.eq_ignore_ascii_case(tag.name.as_bytes()));
        let after = b.get(at + from + tag.name.len()).copied();
        let boundary = after.is_none_or(|c| is_space(c) || c == b'>' || c == b'/');
        if !(named && boundary) {
            i = at + 1;
            continue;
        }
        let Some(inner) = scan_tag(&s[at..], from) else {
            break;
        };
        if end_tag {
            if let Some(start) = open.pop() {
                ends.insert(base + start, Some(base + at + inner.len));
            }
            if open.is_empty() {
                return Some(at + inner.len);
            }
        } else {
            match ends.get(&(base + at)) {
                Some(&Some(end)) => {
                    i = end - base;
                    continue;
                }
                Some(None) => break,
                None => open.push(at),
            }
        }
        i = at + inner.len;
    }
    for start in open {
        ends.insert(base + start, None);
    }
    None
}

/// The start tag at the start of `s`: its name, its attributes and its
/// length in bytes, if it is closed.
pub(crate) fn start_tag_at(s: &str) -> Option<(String, Vec<Attr>, usize)> {
    if !s.starts_with('<') || !s.as_bytes().get(1).is_some_and(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let tag = scan_tag(s, 1)?;
    Some((tag.name, tag.attrs, tag.len))
}

/// Whether `name` is a void element.
pub(crate) fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}

/// Offset of the end tag of `name` in `s`, in any case.
pub(crate) fn end_tag_in(s: &str, name: &str) -> Option<usize> {
    find_end_tag(s, name)
}

struct Tag {
    name: String,
    attrs: Vec<Attr>,
    self_closing: bool,
    /// Bytes from `<` to the end of `>`.
    len: usize,
}

/// Scans a tag whose name begins at `from`. Returns None when the tag is not
/// closed before the end of the input.
fn scan_tag(s: &str, from: usize) -> Option<Tag> {
    let b = s.as_bytes();
    let mut i = from;
    while i < b.len() && !is_space(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let name = s[from..i].to_ascii_lowercase();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut self_closing = false;
    loop {
        while i < b.len() && (is_space(b[i]) || b[i] == b'/') {
            self_closing = b[i] == b'/';
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
        self_closing = false;
        // Attribute name: the first character may be `=`.
        let start = i;
        i += 1;
        while i < b.len() && !is_space(b[i]) && !matches!(b[i], b'/' | b'>' | b'=') {
            i += 1;
        }
        let attr_name = s[start..i].to_ascii_lowercase();
        while i < b.len() && is_space(b[i]) {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && is_space(b[i]) {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vstart = i + 1;
                let vend = vstart + s[vstart..].bytes().position(|c| c == q)?;
                value = decode(&s[vstart..vend], true);
                i = vend + 1;
            } else {
                let vstart = i;
                while i < b.len() && !is_space(b[i]) && b[i] != b'>' {
                    i += 1;
                }
                value = decode(&s[vstart..i], true);
            }
        }
        if !attrs.iter().any(|a| a.name == attr_name) {
            attrs.push(Attr {
                name: attr_name,
                value,
            });
        }
    }
    Some(Tag {
        name,
        attrs,
        self_closing,
        len: i,
    })
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// Offset of `</name` (any case) followed by a space, `/` or `>`.
fn find_end_tag(s: &str, name: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(off) = s[i..].find("</") {
        let at = i + off;
        let after = at + 2 + name.len();
        if after <= b.len()
            && b[at + 2..after].eq_ignore_ascii_case(name.as_bytes())
            && (after == b.len() || is_space(b[after]) || matches!(b[after], b'/' | b'>'))
        {
            return Some(at);
        }
        i = at + 2;
    }
    None
}

/// Decodes character references. In attribute values a legacy reference
/// without semicolon followed by `=` or an alphanumeric stays literal.
pub fn decode(s: &str, in_attr: bool) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match reference(&rest[1..], in_attr) {
            Some((text, len)) => {
                out.push_str(&text);
                rest = &rest[1 + len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reads a reference after `&`; returns its text and the bytes consumed.
fn reference(s: &str, in_attr: bool) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if b.first() == Some(&b'#') {
        let hex = matches!(b.get(1), Some(b'x' | b'X'));
        let start = if hex { 2 } else { 1 };
        let mut i = start;
        while i < b.len()
            && (if hex {
                b[i].is_ascii_hexdigit()
            } else {
                b[i].is_ascii_digit()
            })
        {
            i += 1;
        }
        if i == start {
            return None;
        }
        let code = u32::from_str_radix(&s[start..i], if hex { 16 } else { 10 }).unwrap_or(u32::MAX);
        if b.get(i) == Some(&b';') {
            i += 1;
        }
        return Some((numeric(code).to_string(), i));
    }
    let run = b.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
    if run == 0 {
        return None;
    }
    if b.get(run) == Some(&b';')
        && let Some(text) = named(&s[..=run])
    {
        return Some((text.to_string(), run + 1));
    }
    // The longest legacy name that begins the run.
    for len in (1..=run.min(32)).rev() {
        if let Some(text) = named(&s[..len]) {
            let next = b.get(len).copied();
            if in_attr && next.is_some_and(|c| c == b'=' || c.is_ascii_alphanumeric()) {
                return None;
            }
            return Some((text.to_string(), len));
        }
    }
    None
}

fn named(name: &str) -> Option<&'static str> {
    entities::NAMED
        .binary_search_by(|(n, _)| n.as_bytes().cmp(name.as_bytes()))
        .ok()
        .map(|i| entities::NAMED[i].1)
}

fn numeric(code: u32) -> char {
    const C1: [(u32, u32); 27] = [
        (0x80, 0x20AC),
        (0x82, 0x201A),
        (0x83, 0x0192),
        (0x84, 0x201E),
        (0x85, 0x2026),
        (0x86, 0x2020),
        (0x87, 0x2021),
        (0x88, 0x02C6),
        (0x89, 0x2030),
        (0x8A, 0x0160),
        (0x8B, 0x2039),
        (0x8C, 0x0152),
        (0x8E, 0x017D),
        (0x91, 0x2018),
        (0x92, 0x2019),
        (0x93, 0x201C),
        (0x94, 0x201D),
        (0x95, 0x2022),
        (0x96, 0x2013),
        (0x97, 0x2014),
        (0x98, 0x02DC),
        (0x99, 0x2122),
        (0x9A, 0x0161),
        (0x9B, 0x203A),
        (0x9C, 0x0153),
        (0x9E, 0x017E),
        (0x9F, 0x0178),
    ];
    if code == 0 {
        return '\u{fffd}';
    }
    if let Some(&(_, to)) = C1.iter().find(|(from, _)| *from == code) {
        return char::from_u32(to).unwrap_or('\u{fffd}');
    }
    char::from_u32(code).unwrap_or('\u{fffd}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|n| n.element())
            .map(|e| {
                let inner = names(&e.children);
                if inner.is_empty() {
                    e.name.clone()
                } else {
                    format!("{}({})", e.name, inner.join(" "))
                }
            })
            .collect()
    }

    #[test]
    fn optional_end_tags() {
        let t = parse("<ul><li>a<li>b</ul><p>x<p>y");
        assert_eq!(names(&t), ["ul(li li)", "p", "p"]);
    }

    #[test]
    fn obsolete_void_elements() {
        let t =
            parse("<object><param name=a value=b><param name=c value=d></object><frame><frame>");
        assert_eq!(names(&t), ["object(param param)", "frame", "frame"]);
    }

    #[test]
    fn implied_table_parts() {
        let t = parse("<table><tr><td>a<td>b</table>");
        assert_eq!(names(&t), ["table(tbody(tr(td td)))"]);
    }

    #[test]
    fn references() {
        assert_eq!(
            decode("&uuml;ber &amp &lt;&#x201C;&#8222;&#150;", false),
            "über & <“„–"
        );
        assert_eq!(decode("a=1&not=2", true), "a=1&not=2");
        assert_eq!(decode("&notit;", false), "¬it;");
    }

    #[test]
    fn line_feed_after_pre() {
        let t = parse("<pre>\n\nx</pre>");
        let Node::Element(pre) = &t[0] else { panic!() };
        assert_eq!(
            pre.children,
            vec![Node::Text(Text {
                text: "\nx".into(),
                span: 6..8
            })]
        );
    }

    #[test]
    fn spans() {
        let src = "<p>a</p>\n<ul><li>b</ul>";
        let t = parse(src);
        let ul = t[2].element().unwrap();
        assert_eq!(ul.source(src), "<ul><li>b</ul>");
    }
}
