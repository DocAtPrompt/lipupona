//! The document tree: elements, text and comments, each with the span of
//! source bytes it was read from.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(Text),
    Comment(Comment),
    /// Source bytes passed through unread: a block outside the profile
    /// (SPEC §3.3) or an island (§3.6).
    Verbatim(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// Lower-case element name.
    pub name: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    /// From the `<` of the start tag to the end of the end tag.
    pub span: Range<usize>,
    /// The element was closed without an end tag of its own.
    pub implied_end: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    /// Lower-case attribute name.
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub text: String,
    pub span: Range<usize>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    }

    pub fn classes(&self) -> impl Iterator<Item = &str> {
        self.attr("class").unwrap_or("").split_ascii_whitespace()
    }

    /// The source bytes of the element, as stored.
    pub fn source<'a>(&self, src: &'a str) -> &'a str {
        let s = &src[self.span.clone()];
        if self.implied_end { s.trim_end() } else { s }
    }
}

impl Node {
    pub fn element(&self) -> Option<&Element> {
        match self {
            Node::Element(e) => Some(e),
            _ => None,
        }
    }

    /// Text that is whitespace only, in the sense of HTML.
    pub fn is_blank_text(&self) -> bool {
        matches!(self, Node::Text(t) if t.text.chars().all(is_html_space))
    }
}

pub fn is_html_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}
