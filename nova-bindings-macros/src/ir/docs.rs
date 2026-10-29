/// Rust documentation collected from `///` comments or `#[doc = "..."]`.
///
/// Both the raw attributes (re-emitted verbatim on generated items) and a
/// normalized text form are kept. The normalized form is plain text with no
/// foreign-language formatting applied.
#[derive(Clone, Default)]
pub struct Docs {
    /// First paragraph, joined into one line.
    pub summary: Option<String>,
    /// Everything after the first paragraph, line breaks preserved.
    pub description: Option<String>,
    pub raw_attributes: Vec<syn::Attribute>,
}

impl Docs {
    /// Summary and description joined by a blank line.
    pub fn text(&self) -> String {
        match (&self.summary, &self.description) {
            (Some(summary), Some(description)) => format!("{summary}\n\n{description}"),
            (Some(summary), None) => summary.clone(),
            (None, Some(description)) => description.clone(),
            (None, None) => String::new(),
        }
    }
}
