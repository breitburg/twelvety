//! Error types (port of `Errors/*.js`).
//!
//! Upstream uses `BaseError` subclasses carrying an `originalError`. In Rust we
//! use `anyhow` contexts for chaining and typed errors where code needs to
//! match on the kind of error.

use std::fmt;

/// Thrown when `templateContent` is read before the template rendered.
#[derive(Debug, Clone)]
pub struct TemplateContentPrematureUseError(pub String);

impl fmt::Display for TemplateContentPrematureUseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for TemplateContentPrematureUseError {}

/// Thrown when `templateContent` is read on a template that will never render.
#[derive(Debug, Clone)]
pub struct TemplateContentUnrenderedTemplateError(pub String);

impl fmt::Display for TemplateContentUnrenderedTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for TemplateContentUnrenderedTemplateError {}

#[derive(Debug, Clone)]
pub struct UsingCircularTemplateContentReferenceError(pub String);

impl fmt::Display for UsingCircularTemplateContentReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for UsingCircularTemplateContentReferenceError {}

#[derive(Debug, Clone)]
pub struct DuplicatePermalinkOutputError(pub String);

impl fmt::Display for DuplicatePermalinkOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for DuplicatePermalinkOutputError {}

/// `ErrorUtil.isPrematureTemplateContentError`
pub fn is_premature_template_content_error(e: &anyhow::Error) -> bool {
    e.chain()
        .any(|c| c.downcast_ref::<TemplateContentPrematureUseError>().is_some())
}

/// Format an error and its causes the way `ErrorHandler` logs them.
pub fn format_error_chain(e: &anyhow::Error) -> String {
    let mut out = Vec::new();
    for (i, cause) in e.chain().enumerate() {
        let name = if i == 0 { "Error" } else { "Caused by" };
        out.push(format!("{}{}: {}", "  ".repeat(i), name, cause));
    }
    out.join("\n")
}
