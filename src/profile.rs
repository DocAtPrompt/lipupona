//! Profiles (SPEC §3) and the reference profile (Appendix A), with the check
//! that decides what is inside the profile and what is passed through.

use crate::diag::{Code, Diagnostic};
use crate::dom::{Element, Node, Verbatim, is_html_space};
use crate::{html, url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Block,
    Inline,
}

/// Content models of §3.1, with the inner grammars of the structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Model {
    Phrasing,
    Blocks,
    Either,
    Lead,
    Text,
    Void,
    /// Only the named children: `li` in a list, `tr` in a table section,
    /// cells in a row.
    Items(&'static [&'static str]),
    /// `caption`?, `thead`?, `tbody`, `tfoot`?
    Table,
    /// Blocks and `img`, with at most one `figcaption`, first or last.
    Figure,
    /// `summary`, then blocks.
    Details,
    /// Declared per use by the implementation (`template`).
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Weight {
    Core,
    Aside,
}

/// A profile. The only one so far is the reference profile; its tables are
/// internal, and callers see it through plain values.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The implementation's name in `data-<name>-*`.
    implementation: String,
    container_depth: usize,
    list_depth: usize,
    grid_columns: usize,
    callouts: Vec<(&'static str, Weight)>,
}

impl Profile {
    /// The reference profile of Appendix A.
    pub fn reference(implementation: &str) -> Profile {
        Profile {
            implementation: implementation.to_string(),
            container_depth: 2,
            list_depth: 3,
            grid_columns: 6,
            callouts: vec![
                ("note", Weight::Core),
                ("tip", Weight::Aside),
                ("important", Weight::Core),
                ("warning", Weight::Core),
                ("caution", Weight::Core),
            ],
        }
    }

    /// The implementation's name in `data-<name>-*`.
    pub fn implementation(&self) -> &str {
        &self.implementation
    }

    /// The maximum column count of the pipe grid.
    pub fn grid_columns(&self) -> usize {
        self.grid_columns
    }

    pub(crate) fn element(&self, name: &str) -> Option<(Kind, Model)> {
        use Kind::*;
        use Model::*;
        Some(match name {
            "h1" | "h2" | "h3" | "h4" | "p" | "summary" => (Block, Phrasing),
            "li" => (Block, Lead),
            "td" | "th" | "caption" | "figcaption" => (Block, Either),
            "pre" => (Block, Text),
            "hr" => (Block, Void),
            "ul" | "ol" => (Block, Items(&["li"])),
            "table" => (Block, Table),
            "thead" | "tbody" | "tfoot" => (Block, Items(&["tr"])),
            "tr" => (Block, Items(&["th", "td"])),
            "blockquote" | "div" | "aside" => (Block, Blocks),
            "figure" => (Block, Figure),
            "details" => (Block, Details),
            "template" => (Block, Any),
            "strong" | "em" | "mark" | "code" | "s" | "sup" | "sub" | "kbd" | "a" | "span"
            | "time" => (Inline, Phrasing),
            "br" | "img" => (Inline, Void),
            _ => return None,
        })
    }

    pub(crate) fn kind(&self, name: &str) -> Option<Kind> {
        self.element(name).map(|(k, _)| k)
    }

    pub(crate) fn model(&self, name: &str) -> Option<Model> {
        self.element(name).map(|(_, m)| m)
    }

    pub(crate) fn is_block(&self, node: &Node) -> bool {
        match node {
            Node::Element(e) => self.kind(&e.name) == Some(Kind::Block),
            Node::Verbatim(v) => v.block,
            _ => false,
        }
    }

    pub(crate) fn weight(&self, callout_type: &str) -> Option<Weight> {
        self.callouts
            .iter()
            .find(|(n, _)| *n == callout_type)
            .map(|(_, w)| *w)
    }

    /// The type of a callout: the class after the marker class `callout`.
    pub(crate) fn callout_type<'e>(&self, el: &'e Element) -> Option<&'e str> {
        let mut classes = el.classes();
        if classes.next() != Some("callout") {
            return None;
        }
        if !matches!(el.name.as_str(), "div" | "aside" | "details") {
            return None;
        }
        classes.next()
    }

    pub(crate) fn is_container(&self, el: &Element) -> bool {
        match el.name.as_str() {
            "div" | "aside" => true,
            "details" => self.callout_type(el).is_some(),
            _ => false,
        }
    }

    fn island_marker(&self) -> String {
        format!("data-{}-rendered", self.implementation)
    }

    pub(crate) fn is_island(&self, el: &Element) -> bool {
        let marker = self.island_marker();
        el.attrs.iter().any(|a| a.name == marker)
    }

    /// Checks the children of `body`. Every top-level block that breaks the
    /// profile becomes a verbatim copy of its source (§3.3, §3.4); islands
    /// become verbatim wherever they stand (§3.6).
    pub(crate) fn check(
        &self,
        src: &str,
        nodes: Vec<Node>,
        diags: &mut Vec<Diagnostic>,
    ) -> Vec<Node> {
        let ctx = Ctx {
            parent: "body",
            model: Model::Blocks,
            container_depth: 0,
            list_depth: 0,
            in_structure: false,
        };
        let mut out = Vec::with_capacity(nodes.len());
        let mut run = Vec::new();
        for node in nodes {
            let phrasing = match &node {
                Node::Text(_) => true,
                Node::Element(e) => self.kind(&e.name) == Some(Kind::Inline) && !self.is_island(e),
                _ => false,
            };
            if phrasing {
                run.push(node);
                continue;
            }
            self.check_top_run(src, std::mem::take(&mut run), &mut out, diags);
            let mut found = Vec::new();
            let source = match &node {
                Node::Element(e) => Some(html::lf(e.source(src))),
                _ => None,
            };
            let node = self.check_node(src, node, &ctx, &mut found);
            let blocking = found.iter().any(|d| d.code.is_blocking());
            diags.extend(found);
            match (blocking, source) {
                (true, Some(text)) => out.push(Node::Verbatim(Verbatim { text, block: true })),
                _ => out.push(node),
            }
        }
        self.check_top_run(src, run, &mut out, diags);
        out
    }

    /// Reports what breaks the profile in a tree that was built rather than
    /// read from stored bytes — from notation, or from typed HTML. Nothing is
    /// replaced. `phrasing` checks the nodes as the content of a paragraph.
    pub(crate) fn diagnose(&self, nodes: &[Node], phrasing: bool) -> Vec<Diagnostic> {
        let ctx = Ctx {
            parent: if phrasing { "p" } else { "body" },
            model: if phrasing {
                Model::Phrasing
            } else {
                Model::Blocks
            },
            container_depth: 0,
            list_depth: 0,
            in_structure: false,
        };
        let mut out = Vec::new();
        for node in nodes {
            self.check_node("", node.clone(), &ctx, &mut out);
        }
        out
    }

    /// Text and inline elements directly in `body` break the constraint that
    /// `body` holds blocks. A run of them is passed through as one piece, from
    /// its first to its last non-blank node.
    fn check_top_run(
        &self,
        src: &str,
        run: Vec<Node>,
        out: &mut Vec<Node>,
        diags: &mut Vec<Diagnostic>,
    ) {
        let span = |n: &Node| match n {
            Node::Text(t) => Some(t.span.clone()),
            Node::Element(e) => Some(e.span.clone()),
            _ => None,
        };
        let content: Vec<&Node> = run.iter().filter(|n| !n.is_blank_text()).collect();
        let (Some(first), Some(last)) = (content.first(), content.last()) else {
            out.extend(run);
            return;
        };
        let (Some(start), Some(end)) = (span(first).map(|s| s.start), span(last).map(|s| s.end))
        else {
            out.extend(run);
            return;
        };
        diags.push(Diagnostic::new(
            Code::ConstraintViolated,
            "text or inline elements directly in <body>",
        ));
        let ctx = Ctx {
            parent: "p",
            model: Model::Phrasing,
            container_depth: 0,
            list_depth: 0,
            in_structure: false,
        };
        for node in run {
            if let Node::Element(_) = node {
                self.check_node(src, node, &ctx, diags);
            }
        }
        let text = src.get(start..end).unwrap_or("");
        let text = html::lf(text);
        let text = text.trim_matches(is_html_space);
        if !text.is_empty() {
            out.push(Node::Verbatim(Verbatim {
                text: text.to_string(),
                block: true,
            }));
        }
    }

    fn check_node(&self, src: &str, node: Node, ctx: &Ctx, out: &mut Vec<Diagnostic>) -> Node {
        match node {
            Node::Element(el) => self.check_element(src, el, ctx, out),
            Node::Text(t) => {
                if !Node::Text(t.clone()).is_blank_text() && !accepts_text(ctx.model) {
                    out.push(Diagnostic::new(
                        Code::ConstraintViolated,
                        format!("text directly in <{}>", ctx.parent),
                    ));
                }
                Node::Text(t)
            }
            other => other,
        }
    }

    fn check_element(
        &self,
        src: &str,
        mut el: Element,
        ctx: &Ctx,
        out: &mut Vec<Diagnostic>,
    ) -> Node {
        if self.is_island(&el) && el.implied_end {
            out.push(Diagnostic::new(
                Code::OutOfProfile,
                format!("island <{}> without its end tag", el.name),
            ));
            return Node::Element(el);
        }
        if self.is_island(&el) {
            if let Some(why) = island_violation(&el) {
                out.push(Diagnostic::new(
                    Code::IslandRejected,
                    format!("<{}>: {why}", el.name),
                ));
            }
            let block = !matches!(ctx.model, Model::Phrasing | Model::Lead | Model::Either);
            return Node::Verbatim(Verbatim {
                text: html::lf(el.source(src)),
                block,
            });
        }
        if el.name == html::TOO_DEEP {
            out.push(Diagnostic::new(
                Code::LimitExceeded,
                format!("nesting deeper than {} elements", html::MAX_DEPTH),
            ));
            return Node::Element(el);
        }
        let Some((kind, model)) = self.element(&el.name) else {
            out.push(Diagnostic::new(
                Code::OutOfProfile,
                format!("element <{}>", el.name),
            ));
            return Node::Element(el);
        };
        if ctx.model == Model::Text {
            out.push(Diagnostic::new(
                Code::OutOfProfile,
                format!("element <{}> in <pre>", el.name),
            ));
            return Node::Element(el);
        }
        self.check_attributes(&el, kind, out);
        self.check_placement(&el, kind, ctx, out);
        if let Some(t) = self.callout_type(&el)
            && let Some(weight) = self.weight(t)
        {
            let ok = match el.name.as_str() {
                "div" => weight == Weight::Core,
                "aside" => weight == Weight::Aside,
                _ => true,
            };
            if !ok {
                out.push(Diagnostic::new(
                    Code::CalloutWeightMismatch,
                    format!("callout {t} as <{}>", el.name),
                ));
            }
        }
        self.check_sequence(&el, model, out);

        let container = self.is_container(&el);
        let list = matches!(el.name.as_str(), "ul" | "ol");
        if container && ctx.in_structure {
            out.push(Diagnostic::new(
                Code::ConstraintViolated,
                format!("container <{}> inside a structure", el.name),
            ));
        }
        let container_depth = ctx.container_depth + container as usize;
        let list_depth = ctx.list_depth + list as usize;
        if container && container_depth > self.container_depth {
            out.push(Diagnostic::new(
                Code::LimitExceeded,
                format!("container depth {container_depth}"),
            ));
        }
        if list && list_depth > self.list_depth {
            out.push(Diagnostic::new(
                Code::LimitExceeded,
                format!("list depth {list_depth}"),
            ));
        }
        let child_ctx = Ctx {
            parent: static_name(&el.name),
            model,
            container_depth,
            list_depth,
            in_structure: !container
                && (ctx.in_structure || kind == Kind::Block && model != Model::Phrasing),
        };
        let children = std::mem::take(&mut el.children);
        el.children = children
            .into_iter()
            .map(|c| self.check_node(src, c, &child_ctx, out))
            .collect();
        Node::Element(el)
    }

    fn check_attributes(&self, el: &Element, kind: Kind, out: &mut Vec<Diagnostic>) {
        for a in &el.attrs {
            let ok = match a.name.as_str() {
                n if n.starts_with("data-") => true,
                "class" | "lang" | "href" | "alt" | "width" | "height" | "colspan" | "rowspan"
                | "datetime" => true,
                "id" => kind == Kind::Block,
                "src" => !url::is_absolute(&a.value),
                "start" => el.name == "ol",
                "open" => el.name == "details",
                "role" => el.name == "div" && a.value == "note",
                _ => false,
            };
            if !ok {
                out.push(Diagnostic::new(
                    Code::OutOfProfile,
                    format!("attribute {}=\"{}\" on <{}>", a.name, a.value, el.name),
                ));
            }
        }
    }

    fn check_placement(&self, el: &Element, kind: Kind, ctx: &Ctx, out: &mut Vec<Diagnostic>) {
        let name = el.name.as_str();
        let required_parent: Option<&[&str]> = match name {
            "li" => Some(&["ul", "ol"]),
            "tr" => Some(&["thead", "tbody", "tfoot"]),
            "td" | "th" => Some(&["tr"]),
            "thead" | "tbody" | "tfoot" | "caption" => Some(&["table"]),
            "summary" => Some(&["details"]),
            "figcaption" => Some(&["figure"]),
            _ => None,
        };
        let ok = match (required_parent, ctx.model) {
            (Some(parents), _) => parents.contains(&ctx.parent),
            (None, Model::Items(_) | Model::Table | Model::Void | Model::Text) => false,
            (None, Model::Any) => true,
            (None, model) => match kind {
                Kind::Block => matches!(
                    model,
                    Model::Blocks | Model::Lead | Model::Either | Model::Figure | Model::Details
                ),
                Kind::Inline => {
                    matches!(model, Model::Phrasing | Model::Lead | Model::Either)
                        || (model == Model::Figure && name == "img")
                }
            },
        };
        if !ok {
            out.push(Diagnostic::new(
                Code::ConstraintViolated,
                format!("<{name}> inside <{}>", ctx.parent),
            ));
        }
    }

    /// The order of children that a content model or grammar prescribes.
    fn check_sequence(&self, el: &Element, model: Model, out: &mut Vec<Diagnostic>) {
        let items: Vec<&Node> = el
            .children
            .iter()
            .filter(|n| !n.is_blank_text() && !matches!(n, Node::Comment(_)))
            .collect();
        let names: Vec<&str> = items
            .iter()
            .map(|n| match n {
                Node::Element(e) => e.name.as_str(),
                _ => "#text",
            })
            .collect();
        let is_phrasing = |n: &&Node| match n {
            Node::Text(_) => true,
            Node::Element(e) => self.kind(&e.name) == Some(Kind::Inline),
            _ => false,
        };
        let ok = match model {
            Model::Void => el.children.is_empty(),
            Model::Lead => {
                let first_block = items.iter().position(|n| !is_phrasing(n));
                first_block.is_none_or(|i| !items[i..].iter().any(is_phrasing))
            }
            Model::Either => {
                !(items.iter().any(is_phrasing) && items.iter().any(|n| !is_phrasing(n)))
            }
            Model::Details => names.first() == Some(&"summary") && !names[1..].contains(&"summary"),
            Model::Figure => {
                let captions: Vec<usize> = names
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| **n == "figcaption")
                    .map(|(i, _)| i)
                    .collect();
                captions.len() <= 1 && captions.iter().all(|&i| i == 0 || i + 1 == names.len())
            }
            Model::Table => {
                let mut rest = names.as_slice();
                for optional in ["caption", "thead"] {
                    if rest.first() == Some(&optional) {
                        rest = &rest[1..];
                    }
                }
                rest == ["tbody"] || rest == ["tbody", "tfoot"]
            }
            _ => true,
        };
        if !ok {
            out.push(Diagnostic::new(
                Code::ConstraintViolated,
                format!("children of <{}> out of order", el.name),
            ));
        }
    }
}

struct Ctx {
    parent: &'static str,
    model: Model,
    container_depth: usize,
    list_depth: usize,
    in_structure: bool,
}

fn accepts_text(model: Model) -> bool {
    matches!(
        model,
        Model::Phrasing | Model::Lead | Model::Either | Model::Text | Model::Any
    )
}

/// Element names of the profile as static strings, for diagnostics.
fn static_name(name: &str) -> &'static str {
    const NAMES: &[&str] = &[
        "h1",
        "h2",
        "h3",
        "h4",
        "p",
        "summary",
        "li",
        "td",
        "th",
        "caption",
        "figcaption",
        "pre",
        "hr",
        "ul",
        "ol",
        "table",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "blockquote",
        "div",
        "aside",
        "figure",
        "details",
        "template",
        "strong",
        "em",
        "mark",
        "code",
        "s",
        "sup",
        "sub",
        "kbd",
        "a",
        "span",
        "time",
        "br",
        "img",
    ];
    NAMES.iter().find(|n| **n == name).copied().unwrap_or("?")
}

/// The whitelist of the reference profile's islands: no scripts, no event
/// attributes, no foreignObject, no reference to another host, no
/// `javascript:`, no `@import`.
fn island_violation(el: &Element) -> Option<String> {
    let mut stack: Vec<&Node> = el.children.iter().collect();
    let check = |e: &Element| -> Option<String> {
        if matches!(e.name.as_str(), "script" | "foreignobject") {
            return Some(format!("<{}>", e.name));
        }
        for a in &e.attrs {
            if a.name.starts_with("on") {
                return Some(format!("event attribute {}", a.name));
            }
            if matches!(a.name.as_str(), "href" | "xlink:href" | "src") {
                let v = a.value.trim().to_ascii_lowercase();
                if v.starts_with("javascript:") {
                    return Some("javascript: reference".into());
                }
                if v.starts_with("//") || (url::has_scheme(&v) && !v.starts_with("data:")) {
                    return Some(format!("reference to another host: {}", a.value));
                }
            }
        }
        None
    };
    if let Some(why) = check(el) {
        return Some(why);
    }
    while let Some(node) = stack.pop() {
        match node {
            Node::Element(e) => {
                if let Some(why) = check(e) {
                    return Some(why);
                }
                stack.extend(e.children.iter());
            }
            Node::Text(t) if t.text.contains("@import") => return Some("@import".into()),
            _ => {}
        }
    }
    None
}
