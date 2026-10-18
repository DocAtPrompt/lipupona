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

/// The name this implementation reserves in `data-<name>-*` (§11).
pub const IMPLEMENTATION: &str = "lipupona";

/// Canonicalises the body of an HTML document under the reference profile
/// (`canon` of the specification). The result ends with one line feed.
pub fn canon(html: &str) -> Output {
    canon::canon(html, &Profile::reference(IMPLEMENTATION))
}

/// Reads notation under the reference profile and returns canonical HTML
/// (`html` of the specification). The result ends with one line feed.
pub fn html(notation: &str) -> Output {
    notation::html(notation, &Profile::reference(IMPLEMENTATION))
}

/// Stores a tree that a caller built, such as a converter, under the
/// reference profile: canonical HTML, with what breaks the profile. The
/// result ends with one line feed.
pub fn store(nodes: Vec<dom::Node>) -> Output {
    canon::store(nodes, &Profile::reference(IMPLEMENTATION))
}

/// What in `nodes` breaks the reference profile, for nodes that stand in
/// `body` or, with `phrasing`, in a paragraph.
pub fn check(nodes: &[dom::Node], phrasing: bool) -> Vec<Diagnostic> {
    Profile::reference(IMPLEMENTATION).diagnose(nodes, phrasing)
}

/// The display of HTML under the reference profile: its one canonical
/// notation. The result ends with one line feed.
pub fn display(html: &str) -> Output {
    notation::display(html, &Profile::reference(IMPLEMENTATION))
}

/// The spans of notation for an editor under the reference profile (SPEC
/// §5.14): markup, content, line breaks, separators and invisible
/// characters, tiling the notation in order. Notation that is not
/// canonical is split as the reader sees it.
pub fn spans(notation: &str) -> Vec<Span> {
    notation::spans(notation, &Profile::reference(IMPLEMENTATION))
}
