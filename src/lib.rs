//! lipupona — reader, writer and canonicaliser for profiled HTML.
//!
//! The specification is `SPEC.md`, the test suite lives under `tests/`.
//! Implemented so far: canonical serialization (§4) with the profile check
//! (§3). The notation reader and writer (§5, §6) follow.

pub mod canon;
pub mod diag;
pub mod dom;
pub mod html;
pub mod profile;
mod url;

pub use diag::{Code, Diagnostic};
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
