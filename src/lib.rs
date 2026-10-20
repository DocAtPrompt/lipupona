//! lipupona — reader, writer and canonicaliser for profiled HTML.
//!
//! The specification is `SPEC.md`, the test suite lives under `tests/`.
//! Three conversions, each from text to text with diagnostics: [`canon`]
//! (§4, with the profile check of §3), [`html`] (reading notation, §5, §6)
//! and [`display`] (writing it, §5); and for an editor the [`spans`] of
//! notation (§5.14).

pub mod canon;
pub mod diag;
pub mod dom;
pub mod head;
pub mod html;
pub mod notation;
pub mod profile;
mod url;

pub use diag::{Code, Diagnostic};
pub use notation::{Span, SpanKind};
pub use profile::Profile;

/// Text produced by one of the conversions, with what was reported on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// The name this implementation reserves in `data-<name>-*` (§11). The
/// functions below use it; an application with a name of its own calls the
/// methods of [`Profile`] instead: `Profile::reference("konsona").canon(html)`.
pub const IMPLEMENTATION: &str = "lipupona";

/// Canonicalises the body of an HTML document under the reference profile
/// (`canon` of the specification). The result ends with one line feed.
pub fn canon(html: &str) -> Output {
    Profile::reference(IMPLEMENTATION).canon(html)
}

/// Reads notation under the reference profile and returns canonical HTML
/// (`html` of the specification). The result ends with one line feed.
pub fn html(notation: &str) -> Output {
    Profile::reference(IMPLEMENTATION).html(notation)
}

/// Stores a tree that a caller built, such as a converter, under the
/// reference profile: canonical HTML, with what breaks the profile. The
/// result ends with one line feed.
pub fn store(nodes: Vec<dom::Node>) -> Output {
    Profile::reference(IMPLEMENTATION).store(nodes)
}

/// What in `nodes` breaks the reference profile, for nodes that stand in
/// `body` or, with `phrasing`, in a paragraph.
pub fn check(nodes: &[dom::Node], phrasing: bool) -> Vec<Diagnostic> {
    Profile::reference(IMPLEMENTATION).check(nodes, phrasing)
}

/// The display of HTML under the reference profile: its one canonical
/// notation. The result ends with one line feed.
pub fn display(html: &str) -> Output {
    Profile::reference(IMPLEMENTATION).display(html)
}

/// The spans of notation for an editor under the reference profile (SPEC
/// §5.14): markup, content, line breaks, separators and invisible
/// characters, tiling the notation in order. Notation that is not
/// canonical is split as the reader sees it.
pub fn spans(notation: &str) -> Vec<Span> {
    Profile::reference(IMPLEMENTATION).spans(notation)
}

/// A whole document of `head` around `body`, the canonical HTML of a
/// fragment (as a converter returns it), under the reference profile (C15).
pub fn document(head: &head::Head, body: &str) -> Output {
    Profile::reference(IMPLEMENTATION).document(head, body)
}

/// The conversions under a profile, for an implementation whose name is not
/// [`IMPLEMENTATION`] (its islands are `data-<name>-rendered`, §11).
impl Profile {
    /// [`canon`] under this profile.
    pub fn canon(&self, html: &str) -> Output {
        canon::canon(html, self)
    }

    /// [`html`] under this profile.
    pub fn html(&self, notation: &str) -> Output {
        notation::html(notation, self)
    }

    /// [`store`] under this profile.
    pub fn store(&self, nodes: Vec<dom::Node>) -> Output {
        canon::store(nodes, self)
    }

    /// [`check`] under this profile.
    pub fn check(&self, nodes: &[dom::Node], phrasing: bool) -> Vec<Diagnostic> {
        self.diagnose(nodes, phrasing)
    }

    /// [`display`] under this profile.
    pub fn display(&self, html: &str) -> Output {
        notation::display(html, self)
    }

    /// [`spans`] under this profile.
    pub fn spans(&self, notation: &str) -> Vec<Span> {
        notation::spans(notation, self)
    }

    /// [`document`] under this profile.
    pub fn document(&self, head: &head::Head, body: &str) -> Output {
        self.canon(&head.serialize(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_island_belongs_to_the_implementation_named() {
        let island = "<svg data-konsona-rendered=\"mermaid\" viewBox=\"0 0 10 10\"><rect height=\"8\" width=\"8\" x=\"1\" y=\"1\"/></svg>\n";
        let konsona = Profile::reference("konsona").canon(island);
        assert_eq!(konsona.text, island);
        assert!(konsona.diagnostics.is_empty(), "{:?}", konsona.diagnostics);
        assert!(
            canon(island)
                .diagnostics
                .iter()
                .any(|d| d.code.is_blocking())
        );
    }
}
