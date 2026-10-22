//! Diagnostics with the stable codes of SPEC §9.3.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    OutOfProfile,
    LimitExceeded,
    ConstraintViolated,
    HeadingLevelReduced,
    HtmlNotTaken,
    UnregisteredCallout,
    CalloutWeightMismatch,
    AttributeConflict,
    Comment,
    UnclosedComment,
    IslandRejected,
    AlignmentDropped,
    ExternalReference,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::OutOfProfile => "out-of-profile",
            Code::LimitExceeded => "limit-exceeded",
            Code::ConstraintViolated => "constraint-violated",
            Code::HeadingLevelReduced => "heading-level-reduced",
            Code::HtmlNotTaken => "html-not-taken",
            Code::UnregisteredCallout => "unregistered-callout",
            Code::CalloutWeightMismatch => "callout-weight-mismatch",
            Code::AttributeConflict => "attribute-conflict",
            Code::Comment => "comment",
            Code::UnclosedComment => "unclosed-comment",
            Code::IslandRejected => "island-rejected",
            Code::AlignmentDropped => "alignment-dropped",
            Code::ExternalReference => "external-reference",
        }
    }

    /// A code that takes its block out of the profile, so that the block is
    /// passed through as it is.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            Code::OutOfProfile | Code::LimitExceeded | Code::ConstraintViolated
        )
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    /// A short human-readable account of what was found.
    pub message: String,
}

impl Diagnostic {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
