//! What lipupona's converters share (`docs/converters.md`): the report
//! entry, small builders, and the repairs a built tree may need before it is
//! stored — list items whose phrasing does not lead, lists deeper than the
//! profile.

use lipupona::Profile;
use lipupona::dom::{Attr, Element, Node, Text};

/// One entry of a converter's report: a stable code (`docs/converters.md`
/// §4 or the diagnostics of SPEC §9.3) and what it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub code: String,
    pub message: String,
}

impl Entry {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Entry {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

pub fn element(name: &str, attrs: Vec<Attr>) -> Element {
    Element {
        name: name.to_string(),
        attrs,
        children: Vec::new(),
        span: 0..0,
        implied_end: false,
    }
}

pub fn attr(name: &str, value: impl Into<String>) -> Attr {
    Attr {
        name: name.to_string(),
        value: value.into(),
    }
}

pub fn text(s: impl Into<String>) -> Node {
    Node::Text(Text {
        text: s.into(),
        span: 0..0,
    })
}

/// The text of `nodes` and their descendants.
pub fn plain_text(nodes: &[Node]) -> String {
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

/// Whether `src` is absolute in the sense of the profile: a scheme or a
/// leading `/`.
pub fn is_absolute(src: &str) -> bool {
    if src.starts_with('/') {
        return true;
    }
    src.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && src.find(':').is_some_and(|k| {
            src[..k]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        })
}

/// Whether a node is phrasing under `profile`.
pub fn is_phrasing(profile: &Profile, node: &Node) -> bool {
    match node {
        Node::Text(_) | Node::Comment(_) => true,
        Node::Element(e) => profile.is_inline(&e.name),
        Node::Verbatim(v) => !v.block,
    }
}

/// Content of a list item or definition (model *lead*): phrasing may only
/// lead; a run of phrasing after a block becomes a paragraph.
pub fn lead_then_blocks(profile: &Profile, children: &mut Vec<Node>) {
    let mut out = Vec::with_capacity(children.len());
    let mut run: Vec<Node> = Vec::new();
    let mut seen_block = false;
    for node in std::mem::take(children) {
        if is_phrasing(profile, &node) {
            if seen_block {
                run.push(node);
            } else {
                out.push(node);
            }
            continue;
        }
        paragraph_of(&mut run, &mut out);
        seen_block = true;
        out.push(node);
    }
    paragraph_of(&mut run, &mut out);
    *children = out;
}

/// Moves `run`, unless it is blank, into `out` as a paragraph.
pub fn paragraph_of(run: &mut Vec<Node>, out: &mut Vec<Node>) {
    if run.iter().all(|n| n.is_blank_text()) {
        run.clear();
        return;
    }
    let mut p = element("p", Vec::new());
    p.children = std::mem::take(run);
    out.push(Node::Element(p));
}

fn is_list(el: &Element) -> bool {
    matches!(el.name.as_str(), "ul" | "ol")
}

/// Moves the items of lists below level `max` up into level `max`, after
/// the item they stood in. Returns how many outermost lists changed.
pub fn flatten_lists(nodes: &mut [Node], max: usize) -> usize {
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
