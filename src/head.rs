//! The head of a document (SPEC §3.7) and the document around the body
//! (C15–C18).

use unicode_normalization::UnicodeNormalization;

use crate::canon::{escape_attr, escape_text, normalize_attrs, start_tag};
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Element, is_html_space};
use crate::html::Document;
use crate::{css, url};

/// A document's head: its language, the children of `head` in order, and
/// the attributes outside the profile on `html`, `head` and `body`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Head {
    pub lang: Option<String>,
    pub items: Vec<HeadItem>,
    pub html_attrs: Vec<Attr>,
    pub head_attrs: Vec<Attr>,
    pub body_attrs: Vec<Attr>,
}

/// A child of the head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadItem {
    Title(String),
    /// `<meta name="…" content="…">`; the name in lower case.
    Entry {
        name: String,
        value: String,
    },
    /// A stylesheet: `<style>`, its attributes and its CSS as stored (C18).
    Style {
        attrs: Vec<Attr>,
        css: String,
    },
    /// A stylesheet: `<link rel="stylesheet">` and its attributes (C18).
    Link {
        attrs: Vec<Attr>,
    },
    /// Head content outside the profile, as stored (C17).
    Foreign(String),
}

impl HeadItem {
    /// The item as C16–C18 write it, without the line feed after it.
    pub fn html(&self) -> String {
        let mut out = String::new();
        match self {
            HeadItem::Title(t) => {
                out.push_str("<title>");
                escape_text(t, &mut out);
                out.push_str("</title>");
            }
            HeadItem::Entry { name, value } => {
                out.push_str("<meta name=\"");
                escape_attr(name, &mut out);
                out.push_str("\" content=\"");
                escape_attr(value, &mut out);
                out.push_str("\">");
            }
            HeadItem::Style { attrs, css } => {
                out.push_str(&tag("style", attrs));
                out.push_str(css);
                out.push_str("</style>");
            }
            HeadItem::Link { attrs } => out.push_str(&tag("link", attrs)),
            HeadItem::Foreign(s) => out.push_str(s),
        }
        out
    }
}

/// A start tag with its attributes in canonical form (C11, C12).
fn tag(name: &str, attrs: &[Attr]) -> String {
    let mut el = Element {
        name: name.to_string(),
        attrs: attrs.to_vec(),
        children: Vec::new(),
        span: 0..0,
        implied_end: false,
    };
    normalize_attrs(&mut el);
    start_tag(&el)
}

/// The attributes of a stylesheet (§3.7), with a `type` naming `text/css`
/// dropped and `rel` in lower case; None when it has an attribute that a
/// stylesheet may not have, or is a `link` that is not one.
fn stylesheet(attrs: &[Attr], link: bool) -> Option<Vec<Attr>> {
    let mut out = Vec::new();
    let (mut rel, mut href) = (false, false);
    for a in attrs {
        match a.name.as_str() {
            "media" => out.push(a.clone()),
            "type" if a.value.eq_ignore_ascii_case("text/css") => {}
            n if n.starts_with("data-") => out.push(a.clone()),
            "rel" if link && a.value.eq_ignore_ascii_case("stylesheet") => {
                rel = true;
                out.push(Attr {
                    name: "rel".into(),
                    value: "stylesheet".into(),
                });
            }
            "href" if link && !url::is_absolute(&a.value) => {
                href = true;
                out.push(a.clone());
            }
            _ => return None,
        }
    }
    (!link || (rel && href)).then_some(out)
}

/// Whether HTML is a whole document (§4): after a byte order mark and
/// whitespace it begins with a doctype or an `html` start tag.
pub fn is_document(input: &str) -> bool {
    let s = input.strip_prefix('\u{feff}').unwrap_or(input);
    let s = s.trim_start_matches(is_html_space);
    let head: String = s.chars().take(9).collect::<String>().to_ascii_lowercase();
    head.starts_with("<!doctype")
        || head.starts_with("<html")
            && head[5..]
                .chars()
                .next()
                .is_none_or(|c| c == '>' || c == '/' || is_html_space(c))
}

/// A name of an entry (§3.7): an ASCII letter, then ASCII letters, digits,
/// `-`, `_`, `.` and `:`.
pub fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

impl Head {
    /// The head of a parsed document, and what in it is outside the profile.
    pub fn from_document(doc: &Document) -> (Head, Vec<Diagnostic>) {
        let mut diags = Vec::new();
        let mut head = Head::default();
        let mut foreign_attrs = |attrs: &[Attr], on: &str, keep_lang: bool| -> Vec<Attr> {
            let mut out = Vec::new();
            for a in attrs {
                if keep_lang && a.name == "lang" {
                    continue;
                }
                diags.push(Diagnostic::new(
                    Code::OutOfProfile,
                    format!("attribute {} on <{on}>", a.name),
                ));
                out.push(a.clone());
            }
            out
        };
        head.html_attrs = foreign_attrs(&doc.html_attrs, "html", true);
        head.head_attrs = foreign_attrs(&doc.head_attrs, "head", false);
        head.body_attrs = foreign_attrs(&doc.body_attrs, "body", false);
        head.lang = doc
            .html_attrs
            .iter()
            .find(|a| a.name == "lang")
            .map(|a| a.value.nfc().collect());
        let mut titled = false;
        for part in &doc.head {
            let attr = |name: &str| -> Option<&str> {
                let (_, attrs) = part.element.as_ref()?;
                attrs
                    .iter()
                    .find(|a| a.name == name)
                    .map(|a| a.value.as_str())
            };
            let (name, attrs) = match &part.element {
                Some((name, attrs)) => (name.as_str(), attrs.as_slice()),
                None => ("", &[][..]),
            };
            let sheet = match name {
                "style" => stylesheet(attrs, false).map(|attrs| HeadItem::Style {
                    attrs,
                    css: part.text.clone(),
                }),
                "link" => stylesheet(attrs, true).map(|attrs| HeadItem::Link { attrs }),
                _ => None,
            };
            if let Some(sheet) = sheet {
                if let HeadItem::Style { css, .. } = &sheet {
                    for u in css::external_references(css) {
                        let shown: String = u.chars().take(60).collect();
                        diags.push(Diagnostic::new(
                            Code::ExternalReference,
                            format!("{shown} in a stylesheet"),
                        ));
                    }
                }
                head.items.push(sheet);
                continue;
            }
            let item = match name {
                "title" if !titled => {
                    titled = true;
                    let text: String = part.text.nfc().collect();
                    Some(HeadItem::Title(
                        text.split(is_html_space)
                            .filter(|w| !w.is_empty())
                            .collect::<Vec<_>>()
                            .join(" "),
                    ))
                }
                // The charset is UTF-8 (C16): its declarations are replaced.
                "meta" if attr("charset").is_some() => None,
                "meta"
                    if attrs.len() == 2
                        && attr("http-equiv")
                            .is_some_and(|v| v.eq_ignore_ascii_case("content-type"))
                        && attr("content")
                            .is_some_and(|v| v.to_ascii_lowercase().contains("charset")) =>
                {
                    None
                }
                "meta"
                    if attrs.len() == 2
                        && attr("name").is_some_and(is_name)
                        && attr("content").is_some_and(|v| !v.contains('\n')) =>
                {
                    Some(HeadItem::Entry {
                        name: attr("name").unwrap_or("").to_ascii_lowercase(),
                        value: attr("content").unwrap_or("").nfc().collect(),
                    })
                }
                _ => {
                    let first = part.source.lines().next().unwrap_or("");
                    let shown: String = first.chars().take(60).collect();
                    diags.push(Diagnostic::new(
                        Code::OutOfProfile,
                        format!("{shown} in the head"),
                    ));
                    Some(HeadItem::Foreign(part.source.clone()))
                }
            };
            head.items.extend(item);
        }
        (head, diags)
    }

    /// The document around `body`, the body's canonical serialization (C15).
    pub fn serialize(&self, body: &str) -> String {
        let mut html_attrs = self.html_attrs.clone();
        if let Some(lang) = &self.lang {
            html_attrs.push(Attr {
                name: "lang".into(),
                value: lang.clone(),
            });
        }
        let mut out = String::from("<!DOCTYPE html>\n");
        out.push_str(&tag("html", &html_attrs));
        out.push('\n');
        out.push_str(&tag("head", &self.head_attrs));
        out.push_str("\n<meta charset=\"utf-8\">\n");
        for item in &self.items {
            out.push_str(&item.html());
            out.push('\n');
        }
        out.push_str("</head>\n");
        out.push_str(&tag("body", &self.body_attrs));
        out.push('\n');
        out.push_str(body);
        out.push_str("</body>\n</html>\n");
        out
    }
}
