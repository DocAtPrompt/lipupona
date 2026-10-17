//! md2lipupona — Markdown to lipupona (`docs/converters.md` §2).
//!
//! The Markdown is parsed with `pulldown-cmark`, a new tree is built from its
//! events, and the tree is stored through the library as canonical HTML of
//! the reference profile. Foreign markup never reaches the output as it was;
//! everything dropped, changed or guessed goes into the report.
//!
//! Like the core, the converter does no IO: text in, HTML and report out.

use std::collections::HashMap;

use lipupona::dom::{Attr, Comment, Element, Node, Text};
use lipupona::{Code, Profile};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

/// The Markdown dialect to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flavour {
    /// CommonMark alone.
    #[default]
    CommonMark,
    /// CommonMark with GitHub's tables, strikethrough, task lists and
    /// footnotes.
    Gfm,
}

/// One entry of the report: a stable code (`docs/converters.md` §4 or the
/// diagnostics of SPEC §9.3) and what it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub code: String,
    pub message: String,
}

/// The result of a conversion: the body content as canonical HTML (ending
/// with one line feed) and the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub html: String,
    pub report: Vec<Entry>,
}

/// Converts Markdown into a lipupona document.
pub fn convert(markdown: &str, flavour: Flavour) -> Conversion {
    let mut options = Options::empty();
    if flavour == Flavour::Gfm {
        options |= Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES;
    }
    let profile = Profile::reference(lipupona::IMPLEMENTATION);
    let mut b = Builder::new(&profile);
    for event in Parser::new_ext(markdown, options) {
        b.event(event);
    }
    let mut nodes = b.finish();
    let lists = flatten_lists(&mut nodes, profile.list_depth());
    for _ in 0..lists {
        b.report(
            "list-depth-reduced",
            format!("items below level {} moved up", profile.list_depth()),
        );
    }
    let stored = lipupona::store(nodes);
    for d in stored.diagnostics {
        if d.code != Code::Comment {
            b.report(d.code.as_str(), d.message);
        }
    }
    Conversion {
        html: stored.text,
        report: b.entries,
    }
}

/// What an open frame of the tree came from.
enum Origin {
    Markdown,
    /// An element opened by a raw HTML start tag, with that tag as written.
    Raw(String),
    /// The content of a footnote definition, with its label.
    Footnote(String),
    /// The alt text of an image: source and title.
    Image(String, String),
}

struct Frame {
    el: Element,
    origin: Origin,
}

struct Builder<'p> {
    profile: &'p Profile,
    root: Vec<Node>,
    stack: Vec<Frame>,
    entries: Vec<Entry>,
    /// Text of a code block or an HTML block being read.
    code: Option<String>,
    html: Option<String>,
    /// Inside a table's head row.
    in_head: bool,
    /// Footnote definitions in source order, and the number of each label
    /// in the order of first reference.
    notes: Vec<(String, Vec<Node>)>,
    numbers: HashMap<String, usize>,
    /// Names of raw start tags that stayed text: their end tags are text
    /// too, without a second report.
    literal: Vec<String>,
}

fn element(name: &str, attrs: Vec<Attr>) -> Element {
    Element {
        name: name.to_string(),
        attrs,
        children: Vec::new(),
        span: 0..0,
        implied_end: false,
    }
}

fn attr(name: &str, value: impl Into<String>) -> Attr {
    Attr {
        name: name.to_string(),
        value: value.into(),
    }
}

fn text(s: impl Into<String>) -> Node {
    Node::Text(Text {
        text: s.into(),
        span: 0..0,
    })
}

/// Whether `src` is absolute in the sense of the profile: a scheme or a
/// leading `/`.
fn is_absolute(src: &str) -> bool {
    if src.starts_with('/') {
        return true;
    }
    let mut chars = src.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && src.find(':').is_some_and(|k| {
            src[..k]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        })
}

impl<'p> Builder<'p> {
    fn new(profile: &'p Profile) -> Self {
        Builder {
            profile,
            root: Vec::new(),
            stack: Vec::new(),
            entries: Vec::new(),
            code: None,
            html: None,
            in_head: false,
            notes: Vec::new(),
            numbers: HashMap::new(),
            literal: Vec::new(),
        }
    }

    fn report(&mut self, code: &str, message: impl Into<String>) {
        self.entries.push(Entry {
            code: code.to_string(),
            message: message.into(),
        });
    }

    fn open(&mut self, name: &str, attrs: Vec<Attr>) {
        self.open_with(element(name, attrs), Origin::Markdown);
    }

    fn open_with(&mut self, el: Element, origin: Origin) {
        self.stack.push(Frame { el, origin });
    }

    fn push(&mut self, node: Node) {
        let children = match self.stack.last_mut() {
            Some(f) => &mut f.el.children,
            None => &mut self.root,
        };
        if let (Node::Text(t), Some(Node::Text(last))) = (&node, children.last_mut()) {
            last.text.push_str(&t.text);
            return;
        }
        children.push(node);
    }

    fn push_text(&mut self, s: &str) {
        if let Some(Frame {
            origin: Origin::Image(..),
            el,
        }) = self.stack.last_mut()
        {
            // Alt text is plain text.
            if let Some(Node::Text(t)) = el.children.last_mut() {
                t.text.push_str(s);
            } else {
                el.children.push(text(s));
            }
            return;
        }
        self.push(text(s));
    }

    /// Closes the innermost frame that Markdown opened; raw HTML elements
    /// still open inside it stay text (their start tags were never closed).
    fn close(&mut self) {
        while matches!(
            self.stack.last(),
            Some(Frame {
                origin: Origin::Raw(_),
                ..
            })
        ) {
            self.unwrap_raw();
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        match frame.origin {
            Origin::Footnote(label) => self.notes.push((label, frame.el.children)),
            Origin::Image(src, title) => self.image(frame.el, src, title),
            _ => self.push(Node::Element(frame.el)),
        }
    }

    /// A raw element without its end tag: its start tag as text, then its
    /// content.
    fn unwrap_raw(&mut self) {
        let Some(Frame {
            el,
            origin: Origin::Raw(tag),
        }) = self.stack.pop()
        else {
            return;
        };
        self.report("html-not-taken", tag.clone());
        self.push(text(tag));
        for child in el.children {
            self.push(child);
        }
    }

    fn in_link(&self) -> bool {
        self.stack.iter().any(|f| f.el.name == "a")
    }

    fn image(&mut self, alt_el: Element, src: String, title: String) {
        let alt = plain_text(&alt_el.children);
        if !title.is_empty() {
            self.report(
                "dropped-attribute",
                format!("title \"{title}\" of an image"),
            );
        }
        if !is_absolute(&src) {
            let img = element("img", vec![attr("alt", alt), attr("src", src)]);
            self.push(Node::Element(img));
            return;
        }
        self.report("image-not-local", src.clone());
        if self.in_link() {
            self.push(text(alt));
        } else {
            let mut a = element("a", vec![attr("href", src)]);
            a.children.push(text(alt));
            self.push(Node::Element(a));
        }
    }

    fn event(&mut self, event: Event) {
        if let Some(code) = &mut self.code {
            match event {
                Event::Text(t) => code.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let mut content = self.code.take().unwrap_or_default();
                    if content.ends_with('\n') {
                        content.pop();
                    }
                    if !content.is_empty() {
                        self.push(text(content));
                    }
                    self.close();
                }
                _ => {}
            }
            return;
        }
        if let Some(html) = &mut self.html {
            match event {
                Event::Html(t) | Event::Text(t) => html.push_str(&t),
                Event::End(TagEnd::HtmlBlock) => {
                    let html = self.html.take().unwrap_or_default();
                    self.html_block(html.trim_end_matches('\n'));
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => {
                let mut code = element("code", Vec::new());
                code.children.push(text(t.to_string()));
                self.push(Node::Element(code));
            }
            Event::InlineHtml(t) | Event::Html(t) => self.inline_html(&t),
            Event::FootnoteReference(label) => self.footnote_reference(&label),
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.push(Node::Element(element("br", Vec::new()))),
            Event::Rule => self.push(Node::Element(element("hr", Vec::new()))),
            Event::TaskListMarker(done) => {
                if let Some(f) = self.stack.iter_mut().rev().find(|f| f.el.name == "li") {
                    let class = if done { "task done" } else { "task" };
                    f.el.attrs.push(attr("class", class));
                }
            }
            Event::InlineMath(t) | Event::DisplayMath(t) => self.push_text(&t),
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.open("p", Vec::new()),
            Tag::Heading { level, .. } => {
                let mut n = level as usize;
                let deepest = self.profile.deepest_heading();
                if n > deepest {
                    self.report(
                        "heading-level-reduced",
                        format!("h{n} reduced to h{deepest}"),
                    );
                    n = deepest;
                }
                self.open(&format!("h{n}"), Vec::new());
            }
            Tag::BlockQuote(_) => self.open("blockquote", Vec::new()),
            Tag::CodeBlock(kind) => {
                let mut attrs = Vec::new();
                if let CodeBlockKind::Fenced(info) = kind {
                    let classes = info.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !classes.is_empty() {
                        attrs.push(attr("class", classes));
                    }
                }
                self.open("pre", attrs);
                self.code = Some(String::new());
            }
            Tag::HtmlBlock => self.html = Some(String::new()),
            Tag::List(start) => match start {
                Some(n) => {
                    let attrs = match n {
                        1 => Vec::new(),
                        n => vec![attr("start", n.to_string())],
                    };
                    self.open("ol", attrs);
                }
                None => self.open("ul", Vec::new()),
            },
            Tag::Item => self.open("li", Vec::new()),
            Tag::FootnoteDefinition(label) => self.open_with(
                element("li", Vec::new()),
                Origin::Footnote(label.to_string()),
            ),
            Tag::Table(aligns) => {
                if aligns.iter().any(|a| *a != Alignment::None) {
                    self.report("alignment-dropped", "column alignment of a table");
                }
                self.open("table", Vec::new());
            }
            Tag::TableHead => {
                self.in_head = true;
                self.open("thead", Vec::new());
                self.open("tr", Vec::new());
            }
            Tag::TableRow => {
                if !matches!(self.stack.last(), Some(f) if f.el.name == "tbody") {
                    self.open("tbody", Vec::new());
                }
                self.open("tr", Vec::new());
            }
            Tag::TableCell => self.open(if self.in_head { "th" } else { "td" }, Vec::new()),
            Tag::Emphasis => self.open("em", Vec::new()),
            Tag::Strong => self.open("strong", Vec::new()),
            Tag::Strikethrough => self.open("s", Vec::new()),
            Tag::Superscript => self.open("sup", Vec::new()),
            Tag::Subscript => self.open("sub", Vec::new()),
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => {
                let href = match link_type {
                    LinkType::Email => format!("mailto:{dest_url}"),
                    _ => dest_url.to_string(),
                };
                if !title.is_empty() {
                    self.report("dropped-attribute", format!("title \"{title}\" of a link"));
                }
                self.open("a", vec![attr("href", href)]);
            }
            Tag::Image {
                dest_url, title, ..
            } => self.open_with(
                element("img", Vec::new()),
                Origin::Image(dest_url.to_string(), title.to_string()),
            ),
            // Not enabled in these flavours.
            Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => self.open("div", Vec::new()),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.unwrap_first_paragraph();
                if let Some(f) = self.stack.last_mut() {
                    lead_then_blocks(&mut f.el.children);
                }
                self.close();
            }
            TagEnd::TableHead => {
                self.close();
                self.close();
                self.in_head = false;
            }
            TagEnd::Table => {
                if matches!(self.stack.last(), Some(f) if f.el.name == "tbody") {
                    self.close();
                } else {
                    // A table without body rows still has its tbody.
                    self.push(Node::Element(element("tbody", Vec::new())));
                }
                self.close();
            }
            _ => self.close(),
        }
    }

    /// A list item or footnote whose content begins with a paragraph holds
    /// that paragraph's content directly (content model *lead*).
    fn unwrap_first_paragraph(&mut self) {
        while matches!(
            self.stack.last(),
            Some(Frame {
                origin: Origin::Raw(_),
                ..
            })
        ) {
            self.unwrap_raw();
        }
        let Some(f) = self.stack.last_mut() else {
            return;
        };
        if let Some(Node::Element(p)) = f.el.children.first()
            && p.name == "p"
            && p.attrs.is_empty()
            && let Node::Element(p) = f.el.children.remove(0)
        {
            let rest = std::mem::take(&mut f.el.children);
            f.el.children = p.children;
            f.el.children.extend(rest);
        }
    }

    fn footnote_reference(&mut self, label: &str) {
        let next = self.numbers.len() + 1;
        let n = *self.numbers.entry(label.to_string()).or_insert(next);
        let mut a = element("a", vec![attr("href", format!("#fn-{n}"))]);
        a.children.push(text(n.to_string()));
        let mut sup = element("sup", Vec::new());
        sup.children.push(Node::Element(a));
        self.push(Node::Element(sup));
    }

    /// Raw HTML in phrasing: one tag or comment per event.
    fn inline_html(&mut self, raw: &str) {
        if let Some(body) = raw.strip_prefix("<!--").and_then(|b| b.strip_suffix("-->")) {
            self.push(Node::Comment(Comment {
                text: body.to_string(),
                span: 0..0,
            }));
            return;
        }
        if let Some(name) = raw.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim().to_ascii_lowercase();
            if matches!(self.stack.last(), Some(Frame { origin: Origin::Raw(_), el }) if el.name == name)
            {
                self.close_raw();
                return;
            }
            if let Some(k) = self.literal.iter().rposition(|n| *n == name) {
                self.literal.remove(k);
            } else {
                self.report("html-not-taken", raw.to_string());
            }
            self.push_text(raw);
            return;
        }
        let nodes = lipupona::html::parse(raw);
        let el = match nodes.as_slice() {
            [Node::Element(el)] if el.children.is_empty() => Some(el.clone()),
            _ => None,
        };
        let taken = el.filter(|el| {
            let mut probe = el.clone();
            probe.implied_end = false;
            !lipupona::check(&[Node::Element(probe)], true)
                .iter()
                .any(|d| d.code.is_blocking())
        });
        let Some(mut el) = taken else {
            if let Some(Node::Element(el)) = nodes.first() {
                self.literal.push(el.name.clone());
            }
            self.report("html-not-taken", raw.to_string());
            self.push_text(raw);
            return;
        };
        el.span = 0..0;
        el.implied_end = false;
        if matches!(el.name.as_str(), "br" | "img") {
            self.push(Node::Element(el));
        } else {
            self.open_with(el, Origin::Raw(raw.to_string()));
        }
    }

    fn close_raw(&mut self) {
        if let Some(frame) = self.stack.pop() {
            self.push(Node::Element(frame.el));
        }
    }

    /// An HTML block: taken if it is a fragment of profile elements, else
    /// literal text whose line ends are line breaks.
    fn html_block(&mut self, raw: &str) {
        let nodes: Vec<Node> = lipupona::html::parse(raw)
            .into_iter()
            .filter(|n| !n.is_blank_text())
            .collect();
        // Checked where it stands: a container is not taken into a list item.
        let mut probe = nodes.clone();
        for frame in self.stack.iter().rev() {
            let mut wrapper = element(&frame.el.name, Vec::new());
            wrapper.children = probe;
            probe = vec![Node::Element(wrapper)];
        }
        let blocked = lipupona::check(&probe, false)
            .iter()
            .any(|d| d.code.is_blocking());
        if !blocked && !nodes.iter().any(|n| matches!(n, Node::Text(_))) {
            for node in nodes {
                self.push(node);
            }
            return;
        }
        self.report(
            "html-not-taken",
            raw.lines().next().unwrap_or("").to_string(),
        );
        for part in raw.split("\n\n").filter(|p| !p.trim().is_empty()) {
            let mut p = element("p", Vec::new());
            for (k, line) in part.split('\n').enumerate() {
                if k > 0 {
                    p.children.push(Node::Element(element("br", Vec::new())));
                }
                p.children.push(text(line));
            }
            self.push(Node::Element(p));
        }
    }

    /// The tree, with the footnotes at the end.
    fn finish(&mut self) -> Vec<Node> {
        while !self.stack.is_empty() {
            self.close();
        }
        let mut root = std::mem::take(&mut self.root);
        if self.notes.is_empty() && self.numbers.is_empty() {
            return root;
        }
        let mut notes = std::mem::take(&mut self.notes);
        let mut numbered: Vec<(usize, Vec<Node>)> = Vec::new();
        let mut unreferenced = Vec::new();
        for (label, content) in notes.drain(..) {
            match self.numbers.get(&label) {
                Some(&n) => numbered.push((n, content)),
                None => unreferenced.push(content),
            }
        }
        numbered.sort_by_key(|(n, _)| *n);
        let mut next = self.numbers.len() + 1;
        for content in unreferenced {
            numbered.push((next, content));
            next += 1;
        }
        let mut ol = element("ol", vec![attr("class", "footnotes")]);
        for (n, content) in numbered {
            let mut li = element("li", vec![attr("id", format!("fn-{n}"))]);
            li.children = content;
            ol.children.push(Node::Element(li));
        }
        root.push(Node::Element(ol));
        root
    }
}

/// Content of a list item: phrasing may only lead (content model *lead*);
/// a run of phrasing after a block, as a tight list gives it, becomes a
/// paragraph.
fn lead_then_blocks(children: &mut Vec<Node>) {
    let phrasing = |n: &Node| match n {
        Node::Text(_) | Node::Comment(_) => true,
        Node::Element(e) => matches!(
            e.name.as_str(),
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
                | "br"
                | "img"
        ),
        Node::Verbatim(v) => !v.block,
    };
    let mut out = Vec::with_capacity(children.len());
    let mut run: Vec<Node> = Vec::new();
    let mut seen_block = false;
    for node in std::mem::take(children) {
        if phrasing(&node) {
            if seen_block {
                run.push(node);
            } else {
                out.push(node);
            }
            continue;
        }
        flush_run(&mut run, &mut out);
        seen_block = true;
        out.push(node);
    }
    flush_run(&mut run, &mut out);
    *children = out;
}

fn flush_run(run: &mut Vec<Node>, out: &mut Vec<Node>) {
    if run.iter().all(|n| n.is_blank_text()) {
        run.clear();
        return;
    }
    let mut p = element("p", Vec::new());
    p.children = std::mem::take(run);
    out.push(Node::Element(p));
}

/// The text of `nodes` and their descendants.
fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.text),
            Node::Element(e) => out.push_str(&plain_text(&e.children)),
            _ => {}
        }
    }
    out
}

fn is_list(el: &Element) -> bool {
    matches!(el.name.as_str(), "ul" | "ol")
}

/// Moves the items of lists below level `max` up into level `max`, after
/// the item they stood in. Returns how many outermost lists changed.
fn flatten_lists(nodes: &mut [Node], max: usize) -> usize {
    nodes
        .iter_mut()
        .map(|node| match node {
            Node::Element(el) if is_list(el) => usize::from(flatten_in(el, 1, max)),
            Node::Element(el) => flatten_lists(&mut el.children, max),
            _ => 0,
        })
        .sum()
}

/// Flattens what lies below the list `el` at `level`; whether anything
/// moved.
fn flatten_in(el: &mut Element, level: usize, max: usize) -> bool {
    if level >= max {
        return flatten_items(el);
    }
    fn visit(nodes: &mut [Node], level: usize, max: usize) -> bool {
        let mut moved = false;
        for node in nodes {
            if let Node::Element(el) = node {
                moved |= match is_list(el) {
                    true => flatten_in(el, level + 1, max),
                    false => visit(&mut el.children, level, max),
                };
            }
        }
        moved
    }
    visit(&mut el.children, level, max)
}

/// Flattens a list at the deepest level: every list inside one of its items
/// gives its items, flattened in turn, to follow that item. Whether anything
/// moved.
fn flatten_items(list: &mut Element) -> bool {
    let mut moved = false;
    let mut items = Vec::new();
    for node in std::mem::take(&mut list.children) {
        let Node::Element(mut li) = node else {
            items.push(node);
            continue;
        };
        let mut lifted = Vec::new();
        take_lists(&mut li.children, &mut lifted);
        items.push(Node::Element(li));
        for mut inner in lifted {
            moved = true;
            flatten_items(&mut inner);
            items.extend(inner.children);
        }
    }
    list.children = items;
    moved
}

/// Removes every list from `nodes`, at any depth, into `out`.
fn take_lists(nodes: &mut Vec<Node>, out: &mut Vec<Element>) {
    let mut kept = Vec::with_capacity(nodes.len());
    for node in std::mem::take(nodes) {
        match node {
            Node::Element(el) if is_list(&el) => out.push(el),
            Node::Element(mut el) => {
                take_lists(&mut el.children, out);
                kept.push(Node::Element(el));
            }
            other => kept.push(other),
        }
    }
    *nodes = kept;
}
