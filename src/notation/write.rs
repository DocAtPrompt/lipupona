//! Writing the display (SPEC §5): the one canonical notation of a tree,
//! with W1 (read back, or fall back) and W2 (escape by reading).

use unicode_segmentation::UnicodeSegmentation;

use super::block::{self, SP, callout_line};
use super::inline::{self, Inline};
use super::scan::Scan;
use super::syntax::{self, is_token};
use crate::canon;
use crate::dom::{Element, Node};
use crate::profile::{Kind, Model, Profile, Weight};

pub(crate) struct Writer<'p> {
    pub profile: &'p Profile,
}

/// Where a run of phrasing stands, which decides its line breaks and what
/// it must escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A paragraph or the lead of a list item: block openers at line starts
    /// are escaped.
    Paragraph,
    /// The indented content of a phrasing element line.
    Indented,
    /// After `# `: one line; trailing braces are escaped.
    Heading,
    /// Inside the round bracket of an element line: one line.
    Bracket,
    /// A grid cell: one line; `|` and lone `^`, `<` are escaped.
    Cell,
    /// A cell of the cell-per-line form: one line.
    Line,
    /// A callout title: one line.
    Title,
}

impl Mode {
    fn one_line(self) -> bool {
        !matches!(self, Mode::Paragraph | Mode::Indented)
    }
}

fn indent(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| if l.is_empty() { l } else { format!("  {l}") })
        .collect()
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4")
}

/// A fence for content: three or more backticks, one more than the longest
/// run at the start of a line in it.
fn fence(content: &str) -> String {
    // The reader allows indentation before a closing fence.
    let longest = content
        .split('\n')
        .map(|l| {
            l.trim_start_matches(SP)
                .chars()
                .take_while(|&c| c == '`')
                .count()
        })
        .max()
        .unwrap_or(0);
    "`".repeat((longest + 1).max(3))
}

fn raw_block(html: &str) -> Vec<String> {
    let f = fence(html);
    let mut lines = vec![format!("{f}{{=html}}")];
    lines.extend(html.split('\n').map(str::to_string));
    lines.push(f);
    lines
}

impl<'p> Writer<'p> {
    pub(crate) fn display(&self, nodes: &[Node]) -> String {
        let lines = self.blocks(nodes, false);
        if lines.is_empty() {
            return String::new();
        }
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    /// Blocks of one context, one blank line between them.
    fn blocks(&self, children: &[Node], figure: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut prev: Option<&Node> = None;
        let mut i = 0;
        while i < children.len() {
            let node = &children[i];
            if node.is_blank_text() {
                i += 1;
                continue;
            }
            let lines = if self.is_phrasing(node) && !(figure && is_img(node)) {
                // A run of phrasing among blocks (lead content): a paragraph.
                let start = i;
                while i < children.len() && self.is_phrasing(&children[i]) {
                    i += 1;
                }
                let run = &children[start..i];
                i -= 1;
                self.phrasing_lines(run, Mode::Paragraph)
            } else {
                self.block(node, prev, figure)
            };
            if !out.is_empty() {
                out.push(String::new());
            }
            out.extend(lines);
            prev = Some(node);
            i += 1;
        }
        out
    }

    fn is_phrasing(&self, node: &Node) -> bool {
        match node {
            Node::Text(_) => true,
            Node::Element(e) => self.profile.kind(&e.name) == Some(Kind::Inline),
            Node::Verbatim(v) => !v.block,
            Node::Comment(_) => false,
        }
    }

    fn block(&self, node: &Node, prev: Option<&Node>, figure: bool) -> Vec<String> {
        let e = match node {
            Node::Verbatim(v) => return raw_block(&v.text),
            Node::Comment(c) => {
                return format!("<!--{}-->", c.text)
                    .split('\n')
                    .map(str::to_string)
                    .collect();
            }
            Node::Text(_) => {
                return self.phrasing_lines(std::slice::from_ref(node), Mode::Paragraph);
            }
            Node::Element(e) => e,
        };
        match e.name.as_str() {
            "p" => self.paragraph(e, figure),
            n if is_heading(n) => self.heading(e),
            "hr" if e.attrs.is_empty() => vec!["---".into()],
            "ul" | "ol" => self.list(e, prev),
            "blockquote" if e.attrs.is_empty() => self.quote(e),
            "pre" => self.pre(e),
            "table" => self.table(e),
            "div" | "aside" | "details" if self.profile.callout_type(e).is_some() => {
                self.callout(e)
            }
            "img" if figure => vec![self.inline_line(e)],
            _ => self.element_line(e),
        }
    }

    fn paragraph(&self, e: &Element, figure: bool) -> Vec<String> {
        if !e.attrs.is_empty() || e.children.is_empty() {
            return self.element_line(e);
        }
        // In a figure, a paragraph holding only an img would read as the img.
        if figure && e.children.len() == 1 && is_img(&e.children[0]) {
            return self.element_line(e);
        }
        let lines = self.phrasing_lines(&e.children, Mode::Paragraph);
        // W1: a line that opens a block with markup (a comment) cannot be
        // escaped; the paragraph falls back to its element line.
        if self.opens_block(&lines, figure) {
            return self.element_line(e);
        }
        lines
    }

    /// A written line that would end the paragraph: one that opens a block
    /// with markup, or a blank one from a comment's text.
    fn opens_block(&self, lines: &[String], figure: bool) -> bool {
        lines.iter().any(|l| {
            l.trim_matches(SP).is_empty()
                || l.starts_with(SP)
                || block::opener(self.profile, l, figure).is_some()
        })
    }

    fn heading(&self, e: &Element) -> Vec<String> {
        if !e.attrs.is_empty() || e.children.is_empty() || contains_br(&e.children) {
            return self.element_line(e);
        }
        let level = e.name[1..].parse::<usize>().unwrap_or(1);
        vec![format!(
            "{} {}",
            "#".repeat(level),
            self.phrasing(&e.children, Mode::Heading)
        )]
    }

    /// `.name#id.class[attrs]`.
    fn selector(&self, e: &Element) -> String {
        let mut s = format!(".{}", e.name);
        let mut bracket: Vec<String> = Vec::new();
        for a in &e.attrs {
            match a.name.as_str() {
                "id" if is_token(&a.value) => s.push_str(&format!("#{}", a.value)),
                "class" if a.value.split(' ').all(is_token) => {
                    for c in a.value.split(' ') {
                        s.push('.');
                        s.push_str(c);
                    }
                }
                _ => bracket.push(format!("{}={}", a.name, syntax::quote_value(&a.value))),
            }
        }
        if !bracket.is_empty() {
            s.push('[');
            s.push_str(&bracket.join(" "));
            s.push(']');
        }
        s
    }

    /// The general form: an element line with its content (§5.4).
    fn element_line(&self, e: &Element) -> Vec<String> {
        let head = self.selector(e);
        let model = self.profile.model(&e.name).unwrap_or(Model::Blocks);
        let has_block = e.children.iter().any(|n| self.profile.is_block(n));
        match model {
            Model::Void => vec![head],
            _ if e.children.iter().all(|n| n.is_blank_text()) => vec![head],
            Model::Phrasing => {
                let mut lines = vec![head];
                lines.extend(indent(self.phrasing_lines(&e.children, Mode::Indented)));
                lines
            }
            Model::Either if !has_block => {
                vec![format!(
                    "{head}({})",
                    self.phrasing(&e.children, Mode::Bracket)
                )]
            }
            Model::Lead => {
                let split = e
                    .children
                    .iter()
                    .position(|n| self.profile.is_block(n))
                    .unwrap_or(e.children.len());
                let (lead, rest) = e.children.split_at(split);
                let lead_text = if lead.iter().all(|n| n.is_blank_text()) {
                    String::new()
                } else {
                    format!("({})", self.phrasing(lead, Mode::Bracket))
                };
                let mut lines = vec![format!("{head}{lead_text}")];
                lines.extend(indent(self.blocks(rest, false)));
                lines
            }
            Model::Items(_) | Model::Table => {
                let mut lines = vec![head];
                for c in e.children.iter() {
                    match c {
                        Node::Element(c) => lines.extend(indent(self.element_line(c))),
                        Node::Comment(_) => lines.extend(indent(self.block(c, None, false))),
                        _ => {}
                    }
                }
                lines
            }
            _ => {
                let mut lines = vec![head];
                lines.extend(indent(self.blocks(&e.children, e.name == "figure")));
                lines
            }
        }
    }

    fn inline_line(&self, e: &Element) -> String {
        self.phrasing(&[Node::Element(e.clone())], Mode::Indented)
    }

    /// §5.6.
    fn list(&self, e: &Element, prev: Option<&Node>) -> Vec<String> {
        let items: Vec<&Element> = e.children.iter().filter_map(Node::element).collect();
        let after_same = prev
            .and_then(Node::element)
            .is_some_and(|p| p.name == e.name);
        let attrs_ok = e.attrs.iter().all(|a| e.name == "ol" && a.name == "start");
        let items_ok = items.iter().all(|li| {
            li.attrs.is_empty()
                && li
                    .children
                    .iter()
                    .find(|n| !n.is_blank_text())
                    .is_some_and(|n| !self.profile.is_block(n))
        });
        if after_same || !attrs_ok || !items_ok || items.is_empty() {
            return self.element_line(e);
        }
        let start: i64 = e
            .attr("start")
            .and_then(|s| s.trim_matches(SP).parse().ok())
            .unwrap_or(1);
        let mut lines = Vec::new();
        for (k, li) in items.iter().enumerate() {
            let marker = if e.name == "ol" {
                format!("{}. ", start + k as i64)
            } else {
                "- ".into()
            };
            let split = li
                .children
                .iter()
                .position(|n| self.profile.is_block(n))
                .unwrap_or(li.children.len());
            let (lead, rest) = li.children.split_at(split);
            let lead_lines = self.phrasing_lines(lead, Mode::Paragraph);
            if self.opens_block(&lead_lines, false) {
                return self.element_line(e);
            }
            for (n, l) in lead_lines.into_iter().enumerate() {
                if n == 0 {
                    lines.push(format!("{marker}{l}").trim_end_matches(SP).to_string());
                } else {
                    lines.push(if l.is_empty() { l } else { format!("  {l}") });
                }
            }
            let mut first = true;
            for b in rest.iter().filter(|n| !n.is_blank_text()) {
                let nested_list = matches!(b, Node::Element(x) if x.name == "ul" || x.name == "ol");
                if !(first && nested_list) {
                    lines.push(String::new());
                }
                lines.extend(indent(self.block(b, None, false)));
                first = false;
            }
        }
        lines
    }

    fn quote(&self, e: &Element) -> Vec<String> {
        let mut inner = self.blocks(&e.children, false);
        if inner.is_empty() {
            return self.element_line(e);
        }
        if let Some(first) = inner.first_mut()
            && callout_line(first).is_some()
        {
            first.insert(0, '\\');
        }
        prefix_quote(inner)
    }

    /// §7.3: the short form when the callout carries exactly what it says.
    fn callout(&self, e: &Element) -> Vec<String> {
        let Some(t) = self.profile.callout_type(e) else {
            return self.element_line(e);
        };
        let classes: Vec<&str> = e.classes().collect();
        let weight = self.profile.weight(t);
        let fold = e.name == "details";
        let attrs_ok = e.attrs.iter().all(|a| match a.name.as_str() {
            "class" => true,
            "role" => e.name == "div" && a.value == "note",
            "open" => fold,
            _ => false,
        });
        let element_ok = match e.name.as_str() {
            "div" => weight == Some(Weight::Core) && e.attr("role") == Some("note"),
            "aside" => weight == Some(Weight::Aside),
            _ => weight.is_some(),
        };
        if classes.len() != 2 || !attrs_ok || !element_ok {
            return self.element_line(e);
        }
        let children: Vec<&Node> = e.children.iter().filter(|n| !n.is_blank_text()).collect();
        let (title, rest): (Option<&Element>, &[&Node]) = match children
            .first()
            .and_then(|n| n.element())
        {
            Some(s) if fold && s.name == "summary" && s.attrs.is_empty() => {
                (Some(s), &children[1..])
            }
            _ if fold => return self.element_line(e),
            Some(p) if p.name == "p" && p.attrs.len() == 1 && p.attr("class") == Some("title") => {
                (Some(p), &children[1..])
            }
            _ => (None, &children[..]),
        };
        let unsayable =
            |t: &Element| t.children.iter().all(|n| n.is_blank_text()) || contains_br(&t.children);
        if title.is_some_and(unsayable) {
            return self.element_line(e);
        }
        let mark = match (fold, e.attr("open").is_some()) {
            (false, _) => "",
            (true, true) => "+",
            (true, false) => "-",
        };
        let mut first = format!("[!{t}]{mark}");
        if let Some(title) = title
            && !title.children.is_empty()
        {
            first.push(' ');
            first.push_str(&self.phrasing(&title.children, Mode::Title));
        }
        let rest: Vec<Node> = rest.iter().map(|n| (*n).clone()).collect();
        let mut lines = vec![first];
        lines.extend(self.blocks(&rest, false));
        prefix_quote(lines)
    }

    fn pre(&self, e: &Element) -> Vec<String> {
        let fenced = e.attrs.iter().all(|a| {
            a.name == "class"
                && !a.value.contains('`')
                && a.value != "{=html}"
                && a.value != "=html"
        });
        if !fenced {
            return raw_block(
                canon::serialize(&[Node::Element(e.clone())], self.profile).trim_end_matches('\n'),
            );
        }
        let content: String = e
            .children
            .iter()
            .map(|n| match n {
                Node::Text(t) => t.text.as_str(),
                _ => "",
            })
            .collect();
        let f = fence(&content);
        let mut lines = vec![format!("{f}{}", e.attr("class").unwrap_or(""))];
        if !content.is_empty() {
            lines.extend(content.split('\n').map(str::to_string));
        }
        lines.push(f);
        lines
    }

    // -------------------------------------------------------------- tables

    fn table(&self, e: &Element) -> Vec<String> {
        match Grid::of(self.profile, e) {
            Some(g) if e.attrs.is_empty() => {
                let caption_ok = g.caption.is_none_or(|c| {
                    c.attrs.is_empty()
                        && !c.children.iter().any(|n| self.profile.is_block(n))
                        && !contains_br(&c.children)
                });
                if !caption_ok || !g.adds_up {
                    return self.element_line(e);
                }
                let grid_ok = g.width <= self.profile.grid_columns()
                    && g.cells.iter().all(|c| {
                        c.el.attrs
                            .iter()
                            .all(|a| a.name == "colspan" || a.name == "rowspan")
                            && !c.el.children.iter().any(|n| self.profile.is_block(n))
                            && !has_unescapable_pipe(&c.el.children)
                    });
                let mut lines = Vec::new();
                if let Some(c) = g.caption {
                    lines.push(self.phrasing(&c.children, Mode::Paragraph));
                }
                if grid_ok {
                    lines.extend(self.write_grid(&g));
                } else {
                    lines.extend(self.write_cells(&g));
                }
                lines
            }
            _ => self.element_line(e),
        }
    }

    fn cell_text(&self, c: &GridCell) -> String {
        let text = self.phrasing(&c.el.children, Mode::Cell);
        header_prefix(c, text)
    }

    #[allow(clippy::needless_range_loop)] // rows and columns index together
    fn write_grid(&self, g: &Grid) -> Vec<String> {
        let mut slots: Vec<Vec<String>> = vec![vec![String::new(); g.width]; g.rows];
        for c in &g.cells {
            for r in c.row..c.row + c.rowspan {
                for k in c.col..c.col + c.colspan {
                    slots[r][k] = if r == c.row && k == c.col {
                        self.cell_text(c)
                    } else if r == c.row {
                        "<".into()
                    } else {
                        "^".into()
                    };
                }
            }
        }
        // A row that reads as a separator line has its first cell escaped.
        for row in slots.iter_mut() {
            let looks = row.iter().all(|c| {
                let inner = c.trim_start_matches(':').trim_end_matches(':');
                !inner.is_empty() && inner.chars().all(|x| x == '-')
            });
            if looks {
                row[0].insert(0, '\\');
            }
        }
        let widths: Vec<usize> = (0..g.width)
            .map(|k| {
                slots
                    .iter()
                    .map(|row| row[k].graphemes(true).count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let line = |row: &Vec<String>| {
            let mut s = String::from("|");
            for (k, cell) in row.iter().enumerate() {
                let pad = widths[k] - cell.graphemes(true).count();
                s.push_str(&format!(" {cell}{} |", " ".repeat(pad)));
            }
            s
        };
        let mut lines = Vec::new();
        for (r, row) in slots.iter().enumerate() {
            if g.head_rows > 0 && r == g.head_rows {
                let mut s = String::from("|");
                for w in &widths {
                    s.push_str(&"-".repeat(w + 2));
                    s.push('|');
                }
                lines.push(s);
            }
            lines.push(line(row));
        }
        lines
    }

    fn write_cells(&self, g: &Grid) -> Vec<String> {
        let mut lines = vec![format!("|=== {}", g.width)];
        let mut cells: Vec<&GridCell> = g.cells.iter().collect();
        cells.sort_by_key(|c| (c.row, c.col));
        for c in cells {
            if g.head_rows > 0 && c.row == g.head_rows && c.col == first_col(g, c.row) {
                lines.push("|---".into());
            }
            let mut prefix = String::new();
            match (c.colspan > 1, c.rowspan > 1) {
                (true, true) => prefix.push_str(&format!("{}.{}+", c.colspan, c.rowspan)),
                (true, false) => prefix.push_str(&format!("{}+", c.colspan)),
                (false, true) => prefix.push_str(&format!(".{}+", c.rowspan)),
                _ => {}
            }
            let attrs: Vec<String> =
                c.el.attrs
                    .iter()
                    .filter(|a| a.name != "colspan" && a.name != "rowspan")
                    .map(|a| format!("{}={}", a.name, syntax::quote_value(&a.value)))
                    .collect();
            if !attrs.is_empty() {
                prefix.push_str(&format!("[{}]", attrs.join(" ")));
            }
            if c.el.children.iter().any(|n| self.profile.is_block(n)) {
                lines.push(format!("{prefix}a|"));
                lines.extend(indent(self.blocks(&c.el.children, false)));
            } else {
                let text = self.cell_text_line(c);
                lines.push(if text.is_empty() {
                    format!("{prefix}|")
                } else {
                    format!("{prefix}| {text}")
                });
            }
        }
        lines.push("|===".into());
        lines
    }

    /// A cell's content in the cell-per-line form, where `|` needs no escape.
    fn cell_text_line(&self, c: &GridCell) -> String {
        let text = self.phrasing(&c.el.children, Mode::Line);
        header_prefix(c, text)
    }

    // ------------------------------------------------------------ phrasing

    fn phrasing_lines(&self, nodes: &[Node], mode: Mode) -> Vec<String> {
        self.phrasing(nodes, mode)
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    /// Writes phrasing under W1 and W2, and reads it back to make sure.
    fn phrasing(&self, nodes: &[Node], mode: Mode) -> String {
        let one_line = mode.one_line() || needs_one_line(nodes);
        let mut last = String::new();
        for attempt in 0..3 {
            let mut out = Pieces::default();
            self.pieces(nodes, &mut out, one_line, attempt >= 1, false);
            let s = self.escape(out, mode, attempt >= 2);
            if self.reads_back(&s, nodes, mode) {
                return s;
            }
            last = s;
        }
        last
    }

    fn reads_back(&self, s: &str, nodes: &[Node], mode: Mode) -> bool {
        let mut r = Inline::new(self.profile);
        let mut source = s.to_string();
        if mode == Mode::Paragraph {
            // Leading whitespace of a line is not content.
            source = source
                .split('\n')
                .map(|l| l.trim_start_matches(SP))
                .collect::<Vec<_>>()
                .join("\n");
        }
        let read = inline::unwrap_fragments(r.read(&source));
        canonical_phrasing(&read, self.profile) == canonical_phrasing(nodes, self.profile)
    }

    fn pieces(&self, nodes: &[Node], out: &mut Pieces, one_line: bool, long: bool, in_link: bool) {
        for (k, node) in nodes.iter().enumerate() {
            match node {
                Node::Text(t) => {
                    for ch in t.text.chars() {
                        if in_link && (ch == '[' || ch == ']') {
                            out.markup(&format!("\\{ch}"));
                        } else {
                            out.literal(ch);
                        }
                    }
                }
                Node::Comment(c) => out.markup(&format!("<!--{}-->", c.text)),
                Node::Verbatim(v) => out.markup(&v.text),
                Node::Element(e) => {
                    let next = nodes.get(k + 1);
                    self.inline_element(e, next, out, one_line, long, in_link);
                }
            }
        }
    }

    fn inline_element(
        &self,
        e: &Element,
        next: Option<&Node>,
        out: &mut Pieces,
        one_line: bool,
        long: bool,
        in_link: bool,
    ) {
        if e.name == "br" && e.attrs.is_empty() {
            out.markup(if one_line { ".br()" } else { "\n" });
            return;
        }
        let marker = match e.name.as_str() {
            "strong" => Some("**"),
            "em" => Some("*"),
            "mark" => Some("=="),
            "s" => Some("~~"),
            "sup" => Some("^"),
            "sub" => Some("~"),
            _ => None,
        };
        if let (Some(m), false, true) = (marker, long, e.attrs.is_empty()) {
            let mut inner = Pieces::default();
            self.pieces(&e.children, &mut inner, one_line, long, in_link);
            let intraword = !matches!(m, "^" | "~");
            let first = inner.chars.first().copied();
            let last = inner.chars.last().copied();
            let prev_alnum = out.chars.last().is_some_and(|c| c.is_alphanumeric());
            let next_alnum = match next {
                Some(Node::Text(t)) => t.text.chars().next().is_some_and(|c| c.is_alphanumeric()),
                _ => false,
            };
            let ok = first.is_some_and(|c| !c.is_whitespace())
                && last.is_some_and(|c| !c.is_whitespace())
                && (!intraword || (!prev_alnum && !next_alnum))
                && (intraword || !inner.chars.iter().any(|c| c.is_whitespace()));
            if ok {
                out.markup(m);
                out.append(inner);
                out.markup(m);
                return;
            }
        }
        if e.name == "code" && e.attrs.is_empty() && !long {
            let texts: Option<String> = e
                .children
                .iter()
                .map(|n| match n {
                    Node::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            if let Some(content) = texts.filter(|c| !c.is_empty() && !c.contains('\n')) {
                let longest = content.split(|c| c != '`').map(str::len).max().unwrap_or(0);
                let ticks = "`".repeat(longest + 1);
                let pad = content.starts_with('`')
                    || content.ends_with('`')
                    || (content.starts_with(' ')
                        && content.ends_with(' ')
                        && !content.chars().all(|c| c == ' '));
                let sp = if pad { " " } else { "" };
                out.markup(&format!("{ticks}{sp}{content}{sp}{ticks}"));
                return;
            }
        }
        if e.name == "a" && !long && e.attrs.len() == 1 && e.attr("href").is_some() && !in_link {
            let href = e.attr("href").unwrap_or("");
            out.markup("[");
            self.pieces(&e.children, out, one_line, long, true);
            out.markup("](");
            out.markup(&link_path(href));
            out.markup(")");
            return;
        }
        // The long form, which needs a bracket (§5.5.1).
        let selector = self.selector(e);
        let has_bracket = selector.ends_with(']');
        out.markup(&selector);
        if inline::is_void(&e.name) {
            if !has_bracket {
                out.markup("()");
            }
            return;
        }
        if e.children.is_empty() && has_bracket {
            return;
        }
        let mut inner = Pieces::default();
        self.pieces(&e.children, &mut inner, one_line, long, in_link);
        inner.balance_parens();
        out.markup("(");
        out.append(inner);
        out.markup(")");
    }

    /// W2: a backslash before the first character of every construct the
    /// reader would see in literal text, reading on with each escape in place.
    /// An escape changes nothing the reader sees after it, and of what lies
    /// before only the escaped character: the checks run on the text as it
    /// is, and the backslashes are put in at the end.
    fn escape(&self, mut p: Pieces, mode: Mode, aggressive: bool) -> String {
        if mode == Mode::Bracket {
            p.balance_parens();
            // A closing bracket follows: a backslash at the end would escape it.
            if p.chars.last() == Some(&'\\') && p.literal.last() == Some(&true) {
                let n = p.chars.len() - 1;
                p.chars.insert(n, '\\');
                p.literal.insert(n, false);
            }
        }
        let c = p.chars;
        let lit = p.literal;
        let mut scan = Scan::new(&c);
        let mut escaped = vec![false; c.len()];
        let mut i = 0;
        while i < c.len() {
            if !lit[i] {
                i += 1;
                continue;
            }
            let line_start = i == 0 || c[i - 1] == '\n';
            let mut target = None;
            if line_start && mode == Mode::Paragraph {
                let end = c[i..]
                    .iter()
                    .position(|&x| x == '\n')
                    .map_or(c.len(), |k| i + k);
                target = block::opener_escape(self.profile, &c[i..end]).map(|k| i + k);
            }
            if target.is_none() {
                target = inline::construct_at(self.profile, &mut scan, &c, i, c.len());
            }
            if target.is_none() && mode == Mode::Cell && c[i] == '|' {
                target = Some(i);
            }
            if aggressive
                && target.is_none()
                && c[i].is_ascii_punctuation()
                && c[i] != ','
                && c[i] != '\''
            {
                target = Some(i);
            }
            match target.filter(|&k| k < c.len() && lit[k]) {
                Some(k) => {
                    escaped[k] = true;
                    i = k + 1;
                }
                None => i += 1,
            }
        }
        let mut s = String::with_capacity(c.len());
        for (&ch, &esc) in c.iter().zip(&escaped) {
            if esc {
                s.push('\\');
            }
            s.push(ch);
        }
        match mode {
            Mode::Cell => {
                if s == "^" || s == "<" || s.starts_with("= ") || s == "=" {
                    s.insert(0, '\\');
                }
            }
            Mode::Line => {
                if s.starts_with("= ") || s == "=" {
                    s.insert(0, '\\');
                }
            }
            Mode::Heading => {
                if s.ends_with('}')
                    && let Some(open) = s.rfind(" {")
                    && syntax::brace_attributes(&s[open + 1..]).is_some()
                {
                    s.insert(open + 1, '\\');
                }
            }
            _ => {}
        }
        s
    }
}

/// A `<br>` anywhere in the phrasing, which a one-line form cannot hold.
fn contains_br(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Element(e) => e.name == "br" || contains_br(&e.children),
        _ => false,
    })
}

/// A header cell among body rows begins with `= `; an empty one is `=`.
fn header_prefix(c: &GridCell, text: String) -> String {
    match (c.el.name == "th" && !c.in_head, text.is_empty()) {
        (false, _) => text,
        (true, true) => "=".into(),
        (true, false) => format!("= {text}"),
    }
}

fn is_img(node: &Node) -> bool {
    matches!(node, Node::Element(e) if e.name == "img")
}

fn prefix_quote(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect()
}

/// §5.13: two adjacent breaks, or a break at the start or end — seen
/// through inline elements, since their boundaries make no line.
fn needs_one_line(nodes: &[Node]) -> bool {
    fn flatten(nodes: &[Node], out: &mut Vec<bool>) {
        for n in nodes {
            match n {
                Node::Text(t) if t.text.is_empty() => {}
                Node::Element(e) if e.name == "br" => out.push(true),
                Node::Element(e) if !e.children.is_empty() => flatten(&e.children, out),
                _ => out.push(false),
            }
        }
    }
    let mut atoms = Vec::new();
    flatten(nodes, &mut atoms);
    atoms.first() == Some(&true)
        || atoms.last() == Some(&true)
        || atoms.windows(2).any(|w| w[0] && w[1])
}

/// The path of a link as stored, with `\` and an unbalanced `)` escaped.
fn link_path(href: &str) -> String {
    let mut depth = 0i32;
    let mut balanced = true;
    for ch in href.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    balanced = false;
                }
            }
            _ => {}
        }
    }
    balanced &= depth == 0;
    let mut s = String::new();
    for ch in href.chars() {
        if ch == '\\' || (!balanced && ch == ')') {
            s.push('\\');
        }
        s.push(ch);
    }
    s
}

/// A `|` that no backslash can take in a grid cell: in a code span or in an
/// attribute value (SPEC §5.9).
fn has_unescapable_pipe(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Element(e) if e.name == "code" => e
            .children
            .iter()
            .any(|c| matches!(c, Node::Text(t) if t.text.contains('|'))),
        Node::Element(e) => {
            e.attrs.iter().any(|a| a.value.contains('|')) || has_unescapable_pipe(&e.children)
        }
        _ => false,
    })
}

/// Phrasing as canonical HTML, for comparing what was written with what
/// reads back.
fn canonical_phrasing(nodes: &[Node], profile: &Profile) -> String {
    let mut wrapped = vec![inline::element("p", Vec::new(), nodes.to_vec())];
    canon::normalize(&mut wrapped, profile);
    canon::serialize(&wrapped, profile)
}

#[derive(Default)]
struct Pieces {
    chars: Vec<char>,
    literal: Vec<bool>,
}

impl Pieces {
    fn literal(&mut self, ch: char) {
        self.chars.push(ch);
        self.literal.push(true);
    }

    fn markup(&mut self, s: &str) {
        for ch in s.chars() {
            self.chars.push(ch);
            self.literal.push(false);
        }
    }

    fn append(&mut self, other: Pieces) {
        self.chars.extend(other.chars);
        self.literal.extend(other.literal);
    }

    /// Literal brackets inside a round bracket are escaped when they do not
    /// balance.
    fn balance_parens(&mut self) {
        let mut depth = 0i32;
        let mut ok = true;
        for (ch, lit) in self.chars.iter().zip(&self.literal) {
            if !lit {
                continue;
            }
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    ok &= depth >= 0;
                }
                _ => {}
            }
        }
        if ok && depth == 0 {
            return;
        }
        let mut chars = Vec::new();
        let mut literal = Vec::new();
        for (ch, lit) in self.chars.iter().zip(&self.literal) {
            if *lit && (*ch == '(' || *ch == ')') {
                chars.push('\\');
                literal.push(false);
                chars.push(*ch);
                literal.push(false);
            } else {
                chars.push(*ch);
                literal.push(*lit);
            }
        }
        self.chars = chars;
        self.literal = literal;
    }
}

// ------------------------------------------------------------------ grid

struct GridCell<'e> {
    el: &'e Element,
    row: usize,
    col: usize,
    colspan: usize,
    rowspan: usize,
    in_head: bool,
}

struct Grid<'e> {
    caption: Option<&'e Element>,
    cells: Vec<GridCell<'e>>,
    rows: usize,
    width: usize,
    head_rows: usize,
    adds_up: bool,
}

fn first_col(g: &Grid, row: usize) -> usize {
    g.cells
        .iter()
        .filter(|c| c.row == row)
        .map(|c| c.col)
        .min()
        .unwrap_or(0)
}

impl<'e> Grid<'e> {
    /// The positions of a table's cells; None for what neither the grid nor
    /// the cell-per-line form can say (a `tfoot`, a `td` in a header row).
    fn of(profile: &Profile, table: &'e Element) -> Option<Grid<'e>> {
        if table
            .children
            .iter()
            .any(|n| !n.is_blank_text() && n.element().is_none())
        {
            return None;
        }
        let parts: Vec<&Element> = table.children.iter().filter_map(Node::element).collect();
        let mut caption = None;
        let mut head = None;
        let mut body = None;
        for p in parts {
            match p.name.as_str() {
                "caption" if caption.is_none() && head.is_none() && body.is_none() => {
                    caption = Some(p)
                }
                "thead" if head.is_none() && body.is_none() && p.attrs.is_empty() => head = Some(p),
                "tbody" if body.is_none() && p.attrs.is_empty() => body = Some(p),
                _ => return None,
            }
        }
        let body = body?;
        let mut rows: Vec<(&Element, bool)> = Vec::new();
        for (section, in_head) in [(head, true), (Some(body), false)] {
            if let Some(s) = section {
                for tr in s.children.iter().filter(|n| !n.is_blank_text()) {
                    let tr = tr
                        .element()
                        .filter(|t| t.name == "tr" && t.attrs.is_empty())?;
                    rows.push((tr, in_head));
                }
            }
        }
        let head_rows = rows.iter().filter(|(_, h)| *h).count();
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        let mut cells = Vec::new();
        for (r, (tr, in_head)) in rows.iter().enumerate() {
            let mut col = 0;
            for cell in tr.children.iter().filter(|n| !n.is_blank_text()) {
                let el = cell
                    .element()
                    .filter(|c| c.name == "td" || c.name == "th")?;
                if *in_head && el.name != "th" {
                    return None;
                }
                // A header cell with blocks among body rows has no form.
                if !*in_head && el.name == "th" && el.children.iter().any(|n| profile.is_block(n)) {
                    return None;
                }
                while occupied
                    .get(r)
                    .and_then(|row| row.get(col))
                    .copied()
                    .unwrap_or(false)
                {
                    col += 1;
                }
                let span = |name: &str| {
                    el.attr(name)
                        .and_then(|v| v.trim_matches(SP).parse::<usize>().ok())
                        .unwrap_or(1)
                        .max(1)
                };
                let (colspan, rowspan) = (span("colspan"), span("rowspan"));
                for rr in r..r + rowspan {
                    while occupied.len() <= rr {
                        occupied.push(Vec::new());
                    }
                    for cc in col..col + colspan {
                        if occupied[rr].len() <= cc {
                            occupied[rr].resize(cc + 1, false);
                        }
                        occupied[rr][cc] = true;
                    }
                }
                cells.push(GridCell {
                    el,
                    row: r,
                    col,
                    colspan,
                    rowspan,
                    in_head: *in_head,
                });
                col += colspan;
            }
        }
        let width = occupied.iter().map(Vec::len).max().unwrap_or(0);
        let adds_up = occupied.len() == rows.len()
            && occupied
                .iter()
                .all(|row| row.len() == width && row.iter().all(|&x| x))
            && head_rows < rows.len()
            && cells
                .iter()
                .all(|c| c.in_head || c.row + c.rowspan > head_rows)
            && cells
                .iter()
                .all(|c| !c.in_head || c.row + c.rowspan <= head_rows);
        Some(Grid {
            caption,
            cells,
            rows: rows.len(),
            width,
            head_rows,
            adds_up,
        })
    }
}
