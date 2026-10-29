use syn::{Attribute, Expr, ExprLit, Lit, Meta};

use crate::ir::Docs;

/// Collects `///` comments and `#[doc = "..."]` attributes.
pub fn parse_docs(attrs: &[Attribute]) -> Docs {
    let mut lines = Vec::new();
    let mut raw_attributes = Vec::new();

    for attr in attrs.iter().filter(|attr| attr.path().is_ident("doc")) {
        raw_attributes.push(attr.clone());
        if let Meta::NameValue(meta) = &attr.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &meta.value
        {
            let text = text.value();
            // `split`, not `lines`: an empty `///` must still yield a blank line.
            for line in text.split('\n') {
                // `/// text` is stored as " text".
                lines.push(
                    line.strip_prefix(' ')
                        .unwrap_or(line)
                        .trim_end()
                        .to_string(),
                );
            }
        }
    }

    let (summary, description) = split_paragraphs(&lines);
    Docs {
        summary,
        description,
        raw_attributes,
    }
}

fn split_paragraphs(lines: &[String]) -> (Option<String>, Option<String>) {
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let Some(start) = start else {
        return (None, None);
    };
    let lines = &lines[start..];
    let end = lines
        .iter()
        .position(|line| line.trim().is_empty())
        .unwrap_or(lines.len());

    let summary = lines[..end]
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let description = lines[end..].join("\n").trim().to_string();

    (
        Some(summary),
        (!description.is_empty()).then_some(description),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn splits_summary_and_description() {
        let item: syn::ItemStruct = parse_quote! {
            /// Correlation Trend Indicator (CTI).
            /// Second summary line.
            ///
            /// Measures correlation.
            ///
            /// - item
            #[derive(Clone)]
            struct Cti;
        };
        let docs = parse_docs(&item.attrs);
        assert_eq!(
            docs.summary.as_deref(),
            Some("Correlation Trend Indicator (CTI). Second summary line.")
        );
        assert_eq!(
            docs.description.as_deref(),
            Some("Measures correlation.\n\n- item")
        );
        assert_eq!(docs.raw_attributes.len(), 6);
    }

    #[test]
    fn keeps_indentation_inside_code_blocks() {
        let item: syn::ItemStruct = parse_quote! {
            /// Summary.
            ///
            /// ```text
            ///   indented
            /// ```
            struct Cti;
        };
        let docs = parse_docs(&item.attrs);
        assert_eq!(
            docs.description.as_deref(),
            Some("```text\n  indented\n```")
        );
    }

    #[test]
    fn empty_when_undocumented() {
        let item: syn::ItemStruct = parse_quote!(
            struct Cti;
        );
        let docs = parse_docs(&item.attrs);
        assert!(docs.raw_attributes.is_empty());
        assert!(docs.summary.is_none());
        assert_eq!(docs.text(), "");
    }
}
