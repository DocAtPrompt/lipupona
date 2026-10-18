//! html2lipupona — foreign HTML to lipupona (`docs/converters.md` §3).
//!
//! The HTML is read with the library's parser, and a new tree is built from
//! it by the table of §3.2: what the profile knows is kept with the
//! attributes it allows, what it does not is renamed, unwrapped or dropped,
//! and every such change goes into the report. The tree is stored through
//! the library as canonical HTML of the reference profile.
//!
//! In mode *article* the regions of a web page that are not its content are
//! stripped first (`article`), each kept in the report with its content.
//!
//! Like the core, the converter does no IO: text in, HTML and report out.

mod article;

use lipupona::dom::{Attr, Element, Node};
use lipupona::head::{Head, HeadItem};
use lipupona::{Code, Profile};
pub use lipupona_convert::Entry;
use lipupona_convert::{
    attr, element, flatten_lists, is_absolute, is_phrasing, lead_then_blocks, paragraph_of,
    plain_text, text,
};

/// What to keep (`docs/converters.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// HTML written by an editor: only what is never content is removed.
    #[default]
    Export,
    /// A web page to keep: navigation, sidebars, comments and the like are
    /// stripped as well, and reported with their content.
    Article,
}

/// What belongs in the document's head, for the caller to write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    pub title: Option<String>,
    pub lang: Option<String>,
    /// Entries of the head (SPEC §3.7): the page's metadata of its content,
    /// name and value, in order (`docs/converters.md` §3.2).
    pub entries: Vec<(String, String)>,
}

impl Meta {
    /// The head these values make (`docs/converters.md` §3.2): the title,
    /// `lang`, the entries in their order. Its report is empty, since every
    /// value was read as a head reads it; it is there to match md2lipupona.
    pub fn head(&self) -> (Head, Vec<Entry>) {
        let mut head = Head {
            lang: self.lang.clone(),
            ..Head::default()
        };
        head.items.extend(self.title.clone().map(HeadItem::Title));
        head.items
            .extend(self.entries.iter().map(|(name, value)| HeadItem::Entry {
                name: name.clone(),
                value: value.clone(),
            }));
        (head, Vec::new())
    }
}

/// The result of a conversion: the body content as canonical HTML (ending
/// with one line feed), the report, and the values for the head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub html: String,
    pub report: Vec<Entry>,
    pub meta: Meta,
}

/// Elements that are never content: removed without a report.
const NEVER: &[&str] = &[
    "script", "style", "meta", "link", "noscript", "template", "head", "title", "base",
];

/// Form controls: dropped, reported.
const CONTROLS: &[&str] = &[
    "input", "button", "select", "textarea", "option", "optgroup", "datalist", "output", "meter",
    "progress",
];

/// Wrappers without meaning: unwrapped without a report.
const WRAPPERS: &[&str] = &[
    "html", "body", "div", "span", "section", "article", "main", "header", "footer", "nav",
    "hgroup", "address", "form", "fieldset", "label", "legend", "tbody",
];

/// Embedded objects: a link to their source.
const EMBEDS: &[&str] = &["iframe", "video", "audio", "embed", "object"];

/// Converts foreign HTML into a lipupona document.
pub fn convert(html: &str, mode: Mode) -> Conversion {
    let profile = Profile::reference(lipupona::IMPLEMENTATION);
    let nodes = lipupona::html::parse(html);
    let mut targets = Vec::new();
    link_targets(&nodes, &mut targets);
    let (nodes, stripped) = match mode {
        Mode::Export => (nodes, Vec::new()),
        Mode::Article => article::strip(nodes),
    };
    // A stripped region, converted by the rules of export mode, goes into
    // the report with its content; what that conversion reports does not.
    let mut report: Vec<Entry> = stripped
        .into_iter()
        .map(|region| {
            let (blocks, _) = convert_nodes(&profile, region.nodes, &targets);
            let mut entry = Entry::new("stripped-region", region.label);
            entry.content = Some(store(&profile, blocks, &mut Vec::new()));
            entry
        })
        .collect();
    let (blocks, entries) = convert_nodes(&profile, nodes, &targets);
    report.extend(entries);
    let meta = head(html, &mut report);
    Conversion {
        html: store(&profile, blocks, &mut report),
        report,
        meta,
    }
}

/// Flattens lists deeper than the profile and stores the blocks as
/// canonical HTML, reporting both.
fn store(profile: &Profile, mut blocks: Vec<Node>, report: &mut Vec<Entry>) -> String {
    for _ in 0..flatten_lists(profile, &mut blocks) {
        report.push(Entry::new(
            "list-depth-reduced",
            format!("items below level {} moved up", profile.list_depth()),
        ));
    }
    let stored = lipupona::store(blocks);
    for d in stored.diagnostics {
        if d.code != Code::Comment {
            report.push(Entry::new(d.code.as_str(), d.message));
        }
    }
    stored.text
}

/// Converts foreign HTML, in mode *export*, into blocks of the profile for a
/// caller that builds a tree of its own (md2lipupona's HTML blocks), with
/// the report. Lists are not flattened yet and nothing is stored: the
/// caller does both for its whole tree.
pub fn blocks(html: &str) -> (Vec<Node>, Vec<Entry>) {
    let profile = Profile::reference(lipupona::IMPLEMENTATION);
    let nodes = lipupona::html::parse(html);
    let mut targets = Vec::new();
    link_targets(&nodes, &mut targets);
    convert_nodes(&profile, nodes, &targets)
}

/// Converts parsed nodes by the table of §3.2: blocks, and the report.
fn convert_nodes(
    profile: &Profile,
    nodes: Vec<Node>,
    targets: &[String],
) -> (Vec<Node>, Vec<Entry>) {
    let mut c = Converter {
        profile,
        targets: targets.to_vec(),
        entries: Vec::new(),
        summary: Vec::new(),
        in_link: 0,
        open: Vec::new(),
    };
    let flow = c.flow(nodes);
    let blocks = c.blocks_of(flow);
    let mut report: Vec<Entry> = c
        .summary
        .into_iter()
        .map(|(code, name, n)| match n {
            1 => Entry::new(code, name),
            n => Entry::new(code, format!("{name} ({n})")),
        })
        .collect();
    report.extend(c.entries);
    (blocks, report)
}

struct Converter<'p> {
    profile: &'p Profile,
    /// Fragments that links of the document point to (`#…`).
    targets: Vec<String>,
    entries: Vec<Entry>,
    /// Summarised reports: code, name, count, in order of first occurrence.
    summary: Vec<(&'static str, String, usize)>,
    /// Links open around what is being converted.
    in_link: usize,
    /// Kept elements open around what is being converted.
    open: Vec<String>,
}

impl Converter<'_> {
    fn summarise(&mut self, code: &'static str, name: &str) {
        match self
            .summary
            .iter_mut()
            .find(|(c, n, _)| *c == code && n == name)
        {
            Some(entry) => entry.2 += 1,
            None => self.summary.push((code, name.to_string(), 1)),
        }
    }

    fn report(&mut self, code: &str, message: impl Into<String>) {
        self.entries.push(Entry::new(code, message));
    }

    /// Converted content: blocks and phrasing as they come.
    fn flow(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                Node::Text(t) => out.push(Node::Text(t)),
                Node::Element(el) => out.extend(self.element(el)),
                // Comments of a foreign document are never content.
                Node::Comment(_) | Node::Verbatim(_) => {}
            }
        }
        out
    }

    fn element(&mut self, el: Element) -> Vec<Node> {
        let name = el.name.clone();
        let name = name.as_str();
        if NEVER.contains(&name) {
            return Vec::new();
        }
        if CONTROLS.contains(&name) {
            self.summarise("dropped-element", name);
            return Vec::new();
        }
        match name {
            "span" => {
                let wraps = self.style_meaning(&el);
                let children = self.flow(el.children);
                return wrap_in(wraps, children);
            }
            "b" | "strong" if style_of(&el, "font-weight").is_some_and(|w| !is_bold(&w)) => {
                return self.flow(el.children);
            }
            "b" => return self.inline_as(el, "strong"),
            "i" => return self.inline_as(el, "em"),
            "strike" | "del" => return self.inline_as(el, "s"),
            "h5" | "h6" => {
                self.report("heading-level-reduced", format!("{name} reduced to h4"));
                return self.kept(el, "h4");
            }
            // Never a callout (docs/converters.md §3.2).
            "aside" => {
                self.summarise("dropped-element", name);
                return self.flow(el.children);
            }
            "pre" => return self.pre(el),
            "img" => return self.img(el),
            "a" => return self.link(el),
            "table" => return self.table(el),
            _ => {}
        }
        if WRAPPERS.contains(&name) {
            return self.flow(el.children);
        }
        // Parts of a structure outside it (a `tr` without its table, as
        // CommonMark cuts HTML blocks at blank lines) are unwrapped.
        let parents: &[&str] = match name {
            "tr" | "td" | "th" | "thead" | "tfoot" | "caption" => &["table"],
            "li" => &["ul", "ol"],
            "dt" | "dd" => &["dl"],
            "summary" => &["details"],
            "figcaption" => &["figure"],
            _ => &[],
        };
        let inside = self
            .open
            .last()
            .is_some_and(|p| parents.contains(&p.as_str()));
        if !parents.is_empty() && !inside {
            self.summarise("dropped-element", &format!("{name} outside its parent"));
            return self.flow(el.children);
        }
        if EMBEDS.contains(&name) {
            return self.embed(el);
        }
        if self.profile.has(name) {
            return self.kept(el, name);
        }
        // Outside the profile: the text stays.
        self.summarise("dropped-element", name);
        self.flow(el.children)
    }

    /// An element of the profile, under `name`, with the attributes the
    /// profile allows and its content fitted to its content model.
    fn kept(&mut self, mut el: Element, name: &str) -> Vec<Node> {
        let inline = self.profile.is_inline(name);
        self.open.push(name.to_string());
        let children = self.flow(std::mem::take(&mut el.children));
        self.open.pop();
        // Attributes and styles are reported only for what is kept.
        if inline {
            if children.iter().any(|n| !is_phrasing(self.profile, n)) {
                // An inline element cannot hold blocks.
                self.summarise("dropped-element", name);
                return children;
            }
            let wraps = self.style_meaning(&el);
            let attrs = self.attributes(&el, name);
            let mut out = element(name, attrs);
            out.children = wrap_in(wraps, children);
            return vec![Node::Element(out)];
        }
        // A paragraph or heading with nothing in it is left out.
        let children = match name {
            "p" | "h1" | "h2" | "h3" | "h4" => {
                let children = self.phrasing_only(children);
                if is_empty(&children) {
                    return Vec::new();
                }
                children
            }
            _ => children,
        };
        self.drop_style(&el);
        let attrs = self.attributes(&el, name);
        let children = match name {
            "p" | "h1" | "h2" | "h3" | "h4" | "dt" | "summary" => {
                let children = self.phrasing_only(children);
                if name == "p" {
                    return split_paragraph(attrs, children);
                }
                children
            }
            "li" | "dd" => {
                // The first paragraph leads, as in md2lipupona.
                let mut children = children;
                let first = children.iter().position(|n| !n.is_blank_text());
                if let Some(k) = first
                    && matches!(&children[k], Node::Element(p) if p.name == "p" && p.attrs.is_empty())
                    && let Node::Element(p) = children.remove(k)
                {
                    let rest = children.split_off(k);
                    children = p.children;
                    children.extend(rest);
                }
                lead_then_blocks(self.profile, &mut children);
                children
            }
            "td" | "th" | "caption" | "figcaption" => self.either(children),
            "ul" | "ol" => self.items(children, "li", &["li"]),
            "dl" => {
                let items = self.items(children, "dd", &["dt", "dd"]);
                return self.definition_list(attrs, items);
            }
            "figure" => self.figure(children),
            "details" => self.details(children),
            "hr" | "br" => Vec::new(),
            _ => self.blocks_of(children),
        };
        let mut out = element(name, attrs);
        out.children = children;
        vec![Node::Element(out)]
    }

    /// A definition list in the profile's form: the groups from its first
    /// `dt` to its last `dd`; a `dt` no `dd` follows becomes a paragraph in
    /// `strong`, a `dd` no `dt` precedes gives its content as blocks.
    fn definition_list(&mut self, attrs: Vec<Attr>, items: Vec<Node>) -> Vec<Node> {
        let is = |n: &Node, name: &str| matches!(n, Node::Element(e) if e.name == name);
        let first_dt = items.iter().position(|n| is(n, "dt"));
        let last_dd = items.iter().rposition(|n| is(n, "dd"));
        let (from, to) = match (first_dt, last_dd) {
            (Some(f), Some(l)) if f < l => (f, l + 1),
            _ => (items.len(), items.len()),
        };
        let mut out = Vec::new();
        let mut list = element("dl", attrs);
        for (k, item) in items.into_iter().enumerate() {
            let Node::Element(e) = item else {
                continue;
            };
            if (from..to).contains(&k) {
                list.children.push(Node::Element(e));
            } else if e.name == "dt" {
                self.summarise("dropped-element", "dt without dd");
                if !is_empty(&e.children) {
                    let mut strong = element("strong", Vec::new());
                    strong.children = e.children;
                    let mut p = element("p", Vec::new());
                    p.children.push(Node::Element(strong));
                    out.push(Node::Element(p));
                }
            } else {
                self.summarise("dropped-element", "dd without dt");
                out.extend(self.blocks_of(e.children));
            }
            if k + 1 == to && from < to {
                out.push(Node::Element(std::mem::replace(
                    &mut list,
                    element("dl", Vec::new()),
                )));
            }
        }
        out
    }

    /// An inline element renamed: `b` as `strong`.
    fn inline_as(&mut self, mut el: Element, name: &str) -> Vec<Node> {
        el.name = name.to_string();
        self.kept(el, name)
    }

    /// The attributes of a kept element that the profile allows; the rest
    /// is reported once per name.
    fn attributes(&mut self, el: &Element, name: &str) -> Vec<Attr> {
        let block = !self.profile.is_inline(name);
        let mut out = Vec::new();
        for a in &el.attrs {
            let keep = match a.name.as_str() {
                "lang" => true,
                "id" => block && self.targets.contains(&a.value),
                "href" => name == "a",
                "src" | "alt" | "width" | "height" => name == "img",
                "colspan" | "rowspan" => matches!(name, "td" | "th"),
                "start" => name == "ol",
                "datetime" => name == "time",
                "open" => name == "details",
                "style" => continue,
                _ => false,
            };
            if keep {
                out.push(a.clone());
            } else {
                self.summarise("dropped-attribute", &a.name);
            }
        }
        out
    }

    /// The elements that a `style` means (`docs/converters.md` §3.2); the
    /// rest of the style is reported once per property.
    fn style_meaning(&mut self, el: &Element) -> Vec<&'static str> {
        let mut wraps = Vec::new();
        for (prop, value) in style(el) {
            match prop.as_str() {
                "font-weight" => {
                    if is_bold(&value) {
                        wraps.push("strong");
                    }
                }
                "font-style" => {
                    if value.contains("italic") || value.contains("oblique") {
                        wraps.push("em");
                    }
                }
                "text-decoration" | "text-decoration-line" if value.contains("line-through") => {
                    wraps.push("s");
                }
                "vertical-align" if value == "super" => wraps.push("sup"),
                "vertical-align" if value == "sub" => wraps.push("sub"),
                "vertical-align" if value == "baseline" => {}
                _ => self.summarise("dropped-style", &prop),
            }
        }
        wraps
    }

    fn drop_style(&mut self, el: &Element) {
        for (prop, _) in style(el) {
            self.summarise("dropped-style", &prop);
        }
    }

    /// Phrasing of a block whose model is phrasing: blocks inside it give
    /// their phrasing.
    fn phrasing_only(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                Node::Element(e) if !self.profile.is_inline(&e.name) => {
                    if !out.is_empty() {
                        out.push(Node::Element(element("br", Vec::new())));
                    }
                    out.extend(self.phrasing_only(e.children));
                }
                other => out.push(other),
            }
        }
        out
    }

    /// Content of a block that holds blocks: runs of phrasing become
    /// paragraphs, split at runs of `br`.
    fn blocks_of(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::new();
        let mut run = Vec::new();
        for node in nodes {
            if is_phrasing(self.profile, &node) {
                run.push(node);
                continue;
            }
            flush(&mut run, &mut out);
            out.push(node);
        }
        flush(&mut run, &mut out);
        out
    }

    /// Content model *either*: phrasing, or blocks if there are any. A lone
    /// paragraph is its phrasing, as Word puts one into every cell.
    fn either(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        if nodes.iter().all(|n| is_phrasing(self.profile, n)) {
            return nodes;
        }
        let solid: Vec<&Node> = nodes.iter().filter(|n| !n.is_blank_text()).collect();
        if let [Node::Element(p)] = solid.as_slice()
            && p.name == "p"
            && p.attrs.is_empty()
        {
            return p.children.clone();
        }
        self.blocks_of(nodes)
    }

    /// Children of a list: items, with anything else wrapped in an item of
    /// kind `wrap`.
    fn items(&mut self, nodes: Vec<Node>, wrap: &str, allowed: &[&str]) -> Vec<Node> {
        let mut out = Vec::new();
        let mut stray = Vec::new();
        for node in nodes {
            match node {
                Node::Element(e) if allowed.contains(&e.name.as_str()) => {
                    wrap_stray(&mut stray, wrap, &mut out, self.profile);
                    out.push(Node::Element(e));
                }
                other if other.is_blank_text() => {}
                other => stray.push(other),
            }
        }
        wrap_stray(&mut stray, wrap, &mut out, self.profile);
        out
    }

    /// A figure: `img` stands in it directly; other phrasing becomes
    /// paragraphs.
    fn figure(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::new();
        let mut run = Vec::new();
        for node in nodes {
            match node {
                Node::Element(e) if e.name == "img" => {
                    flush(&mut run, &mut out);
                    out.push(Node::Element(e));
                }
                n if is_phrasing(self.profile, &n) => run.push(n),
                n => {
                    flush(&mut run, &mut out);
                    out.push(n);
                }
            }
        }
        flush(&mut run, &mut out);
        out
    }

    /// A details element: its summary first, then blocks.
    fn details(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let (summary, rest): (Vec<Node>, Vec<Node>) = nodes
            .into_iter()
            .partition(|n| matches!(n, Node::Element(e) if e.name == "summary"));
        let mut out: Vec<Node> = summary.into_iter().take(1).collect();
        if out.is_empty() {
            let mut s = element("summary", Vec::new());
            s.children.push(text("Details"));
            out.push(Node::Element(s));
            self.report("dropped-element", "details without summary");
        }
        out.extend(self.blocks_of(rest));
        out
    }

    /// `pre`: text only (C8); a language class of its `code` moves to it.
    fn pre(&mut self, el: Element) -> Vec<Node> {
        let mut class = None;
        for node in &el.children {
            if let Node::Element(code) = node
                && code.name == "code"
            {
                class = code.classes().find_map(|c| {
                    c.strip_prefix("language-")
                        .or_else(|| c.strip_prefix("lang-"))
                        .map(str::to_string)
                });
            }
        }
        let attrs: Vec<Attr> = self
            .attributes(&el, "pre")
            .into_iter()
            .chain(class.map(|c| attr("class", c)))
            .collect();
        self.drop_style(&el);
        let mut pre = element("pre", attrs);
        let content = plain_text(&el.children);
        if !content.is_empty() {
            pre.children.push(text(content));
        }
        vec![Node::Element(pre)]
    }

    fn img(&mut self, el: Element) -> Vec<Node> {
        let src = el.attr("src").unwrap_or("").to_string();
        let alt = el.attr("alt").unwrap_or("").to_string();
        if src.is_empty() {
            self.summarise("dropped-element", "img without src");
            return text_unless_empty(alt);
        }
        if src.starts_with("data:") {
            self.report("image-not-local", src);
            return text_unless_empty(alt);
        }
        if is_absolute(&src) {
            self.report("image-not-local", src.clone());
            if self.in_link > 0 {
                return text_unless_empty(alt);
            }
            let mut a = element("a", vec![attr("href", src)]);
            a.children.push(text(alt));
            return vec![Node::Element(a)];
        }
        let mut attrs = vec![attr("alt", alt)];
        attrs.extend(
            self.attributes(&el, "img")
                .into_iter()
                .filter(|a| a.name != "alt"),
        );
        self.drop_style(&el);
        vec![Node::Element(element("img", attrs))]
    }

    fn link(&mut self, el: Element) -> Vec<Node> {
        let href = el.attr("href").map(str::to_string);
        let Some(href) = href else {
            return self.flow(el.children);
        };
        if href
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("javascript:")
        {
            self.summarise("dropped-attribute", "href");
            return self.flow(el.children);
        }
        self.in_link += 1;
        let out = self.kept(el, "a");
        self.in_link -= 1;
        out
    }

    fn embed(&mut self, el: Element) -> Vec<Node> {
        let src = el
            .attr("src")
            .or_else(|| el.attr("data"))
            .map(str::to_string)
            .or_else(|| {
                el.children.iter().find_map(|n| match n {
                    Node::Element(s) if s.name == "source" => s.attr("src").map(str::to_string),
                    _ => None,
                })
            });
        let Some(src) = src else {
            self.summarise("dropped-element", &el.name);
            return Vec::new();
        };
        self.report("embed-as-link", format!("<{}> {src}", el.name));
        let label = el.attr("title").map_or_else(|| src.clone(), str::to_string);
        let mut a = element("a", vec![attr("href", src)]);
        a.children.push(text(label));
        vec![Node::Element(a)]
    }

    fn table(&mut self, el: Element) -> Vec<Node> {
        let mut caption = None;
        let mut head = Vec::new();
        let mut body = Vec::new();
        let mut foot = Vec::new();
        for node in el.children.iter().cloned() {
            let Node::Element(part) = node else { continue };
            match part.name.as_str() {
                "caption" => caption = Some(part),
                "thead" => head.extend(rows(part.children)),
                "tfoot" => foot.extend(rows(part.children)),
                "tbody" => body.extend(rows(part.children)),
                "tr" => body.push(part),
                _ => {}
            }
        }
        let cells: Vec<&Element> = head
            .iter()
            .chain(&body)
            .chain(&foot)
            .flat_map(|tr| tr.children.iter().filter_map(Node::element))
            .filter(|c| matches!(c.name.as_str(), "td" | "th"))
            .collect();
        let no_cells = cells.is_empty();
        let has_th = cells.iter().any(|c| c.name == "th");
        let with_blocks = cells
            .iter()
            .filter(|c| holds_blocks(self.profile, c))
            .count();
        let layout = el.attr("role") == Some("presentation")
            || cells.len() == 1
            || has_table(&el.children)
            || (!has_th && with_blocks * 2 > cells.len());
        if layout {
            self.report("layout-table-unwrapped", format!("{} cells", cells.len()));
            let mut out = Vec::new();
            if let Some(c) = caption {
                let content = self.flow(c.children);
                out.extend(self.blocks_of(content));
            }
            for cell in head.into_iter().chain(body).chain(foot) {
                for c in cell.children {
                    if let Node::Element(c) = c {
                        let content = self.flow(c.children);
                        out.extend(self.blocks_of(content));
                    }
                }
            }
            return out;
        }
        if head.is_empty()
            && let Some(first) = body.first()
            && first
                .children
                .iter()
                .filter_map(Node::element)
                .all(|c| c.name == "th")
        {
            head.push(body.remove(0));
        }
        self.drop_style(&el);
        let attrs = self.attributes(&el, "table");
        if no_cells && caption.is_none() {
            // Nothing in it.
            return Vec::new();
        }
        let mut table = element("table", attrs);
        self.open.push("table".to_string());
        if let Some(c) = caption {
            table.children.extend(self.kept(c, "caption"));
        }
        for (name, part) in [("thead", head), ("tbody", body), ("tfoot", foot)] {
            if part.is_empty() && name != "tbody" {
                continue;
            }
            let mut section = element(name, Vec::new());
            for tr in part {
                section.children.extend(self.row(tr));
            }
            table.children.push(Node::Element(section));
        }
        self.open.pop();
        vec![Node::Element(table)]
    }

    fn row(&mut self, tr: Element) -> Vec<Node> {
        self.drop_style(&tr);
        let attrs = self.attributes(&tr, "tr");
        let mut row = element("tr", attrs);
        self.open.push("tr".to_string());
        for node in tr.children {
            if let Node::Element(cell) = node
                && matches!(cell.name.as_str(), "td" | "th")
            {
                let name = cell.name.clone();
                row.children.extend(self.kept(cell, &name));
            }
        }
        self.open.pop();
        vec![Node::Element(row)]
    }
}

/// The `tr` among `nodes`.
fn rows(nodes: Vec<Node>) -> Vec<Element> {
    nodes
        .into_iter()
        .filter_map(|n| match n {
            Node::Element(e) if e.name == "tr" => Some(e),
            _ => None,
        })
        .collect()
}

fn has_table(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Element(e) if e.name == "table" => true,
        Node::Element(e) => has_table(&e.children),
        _ => false,
    })
}

/// Whether a cell of the source holds blocks; a lone paragraph does not
/// count, as Word puts one into every cell.
fn holds_blocks(profile: &Profile, cell: &Element) -> bool {
    let blocks: Vec<&Element> = cell
        .children
        .iter()
        .filter_map(Node::element)
        .filter(|e| {
            matches!(e.name.as_str(), "div" | "section" | "article")
                || (profile.has(&e.name) && !profile.is_inline(&e.name))
        })
        .collect();
    !(blocks.is_empty() || matches!(blocks.as_slice(), [p] if p.name == "p"))
}

fn wrap_in(wraps: Vec<&'static str>, children: Vec<Node>) -> Vec<Node> {
    let mut children = children;
    for name in wraps.into_iter().rev() {
        let mut el = element(name, Vec::new());
        el.children = children;
        children = vec![Node::Element(el)];
    }
    children
}

fn wrap_stray(stray: &mut Vec<Node>, wrap: &str, out: &mut Vec<Node>, profile: &Profile) {
    if stray.iter().all(|n| n.is_blank_text()) {
        stray.clear();
        return;
    }
    let mut item = element(wrap, Vec::new());
    item.children = std::mem::take(stray);
    lead_then_blocks(profile, &mut item.children);
    out.push(Node::Element(item));
}

fn text_unless_empty(s: String) -> Vec<Node> {
    match s.is_empty() {
        true => Vec::new(),
        false => vec![text(s)],
    }
}

/// A run of phrasing as paragraphs, split at runs of two or more `br`.
fn flush(run: &mut Vec<Node>, out: &mut Vec<Node>) {
    for part in split_at_breaks(std::mem::take(run)) {
        let mut part = part;
        paragraph_of(&mut part, out);
    }
}

/// A paragraph that holds runs of `br` is several paragraphs.
fn split_paragraph(attrs: Vec<Attr>, children: Vec<Node>) -> Vec<Node> {
    let parts = split_at_breaks(children);
    if parts.is_empty() {
        return vec![Node::Element(element("p", attrs))];
    }
    parts
        .into_iter()
        .map(|part| {
            let mut p = element("p", attrs.clone());
            p.children = part;
            Node::Element(p)
        })
        .collect()
}

/// Whether `nodes` hold neither text nor an image.
fn is_empty(nodes: &[Node]) -> bool {
    nodes.iter().all(|n| match n {
        Node::Text(t) => t.text.trim().is_empty(),
        Node::Element(e) => e.name != "img" && is_empty(&e.children),
        _ => true,
    })
}

fn is_br(n: &Node) -> bool {
    matches!(n, Node::Element(e) if e.name == "br")
}

/// Splits phrasing at runs of two or more `br` (whitespace between them
/// does not count); the parts lose the breaks at their ends.
fn split_at_breaks(nodes: Vec<Node>) -> Vec<Vec<Node>> {
    let mut parts: Vec<Vec<Node>> = vec![Vec::new()];
    let mut k = 0;
    while k < nodes.len() {
        if is_br(&nodes[k]) {
            let mut j = k;
            let mut breaks = 0;
            while j < nodes.len() && (is_br(&nodes[j]) || nodes[j].is_blank_text()) {
                breaks += usize::from(is_br(&nodes[j]));
                j += 1;
            }
            if breaks >= 2 {
                parts.push(Vec::new());
                k = j;
                continue;
            }
        }
        if let Some(last) = parts.last_mut() {
            last.push(nodes[k].clone());
        }
        k += 1;
    }
    parts
        .into_iter()
        .map(|mut p| {
            while p.first().is_some_and(|n| is_br(n) || n.is_blank_text()) {
                p.remove(0);
            }
            while p.last().is_some_and(|n| is_br(n) || n.is_blank_text()) {
                p.pop();
            }
            p
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// The declarations of an element's `style`, names in lower case.
fn style(el: &Element) -> Vec<(String, String)> {
    el.attr("style")
        .unwrap_or("")
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(p, v)| (p.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase()))
        .filter(|(p, _)| !p.is_empty())
        .collect()
}

fn style_of(el: &Element, prop: &str) -> Option<String> {
    style(el)
        .into_iter()
        .find(|(p, _)| p == prop)
        .map(|(_, v)| v)
}

fn is_bold(value: &str) -> bool {
    matches!(value, "bold" | "bolder") || value.parse::<u32>().is_ok_and(|w| w >= 600)
}

/// Fragments that links of the document point to.
fn link_targets(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        if let Node::Element(e) = node {
            if e.name == "a"
                && let Some(fragment) = e.attr("href").and_then(|h| h.strip_prefix('#'))
            {
                out.push(fragment.to_string());
            }
            link_targets(&e.children, out);
        }
    }
}

/// The title and the language of a whole document.
/// The values for the head, read as SPEC §3.7 reads a head: title,
/// language, and the entries that are metadata of the content; every other
/// entry is reported once per name.
fn head(html: &str, report: &mut Vec<Entry>) -> Meta {
    let (head, _) = lipupona::head::Head::from_document(&lipupona::html::parse_document(html));
    let mut meta = Meta {
        lang: head.lang.filter(|l| !l.is_empty()),
        ..Meta::default()
    };
    let mut dropped: Vec<(String, usize)> = Vec::new();
    for item in head.items {
        match item {
            HeadItem::Title(t) if !t.is_empty() && meta.title.is_none() => meta.title = Some(t),
            HeadItem::Entry { name, value } => match name.as_str() {
                "keywords" => meta.entries.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .map(|k| (name.clone(), k.to_string())),
                ),
                "description" | "author" => meta.entries.push((name, value)),
                n if n.starts_with("dcterms.") || n.starts_with("citation_") => {
                    meta.entries.push((name, value))
                }
                _ => match dropped.iter_mut().find(|(n, _)| *n == name) {
                    Some(d) => d.1 += 1,
                    None => dropped.push((name, 1)),
                },
            },
            _ => {}
        }
    }
    for (name, n) in dropped {
        let message = match n {
            1 => format!("meta {name}"),
            n => format!("meta {name} ({n})"),
        };
        report.push(Entry::new("dropped-element", message));
    }
    meta
}
