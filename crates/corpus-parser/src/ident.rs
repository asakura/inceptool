//! Rust identifier fragment sanitization.

/// Produces a valid Rust identifier fragment from an arbitrary name.
///
/// Lowercases all ASCII letters, replaces runs of non-alphanumeric characters with a
/// single underscore, strips leading/trailing underscores, and prepends `_` if the
/// result would otherwise start with a digit (Rust identifiers cannot start with one).
///
/// The caller must ensure `name` contains at least one ASCII-alphanumeric character;
/// otherwise the returned string will be empty, which is not a valid Rust identifier.
///
/// # Examples
///
/// ```
/// # use inceptool_corpus_parser::to_ident_fragment;
/// assert_eq!(to_ident_fragment("Basic"), "basic");
/// assert_eq!(to_ident_fragment("Hello World"), "hello_world");
/// assert_eq!(to_ident_fragment("foo--bar"), "foo_bar");
/// assert_eq!(to_ident_fragment("  leading  "), "leading");
/// assert_eq!(to_ident_fragment("123"), "_123");
/// ```
#[must_use = "returns the sanitized identifier fragment; original is unchanged"]
pub fn to_ident_fragment(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut last_was_underscore = false;

    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_underscore = false;
        } else if !last_was_underscore {
            result.push('_');
            last_was_underscore = true;
        } else {
            // Collapse consecutive non-alphanumeric characters into a single underscore.
        }
    }

    let trimmed = result.trim_start_matches('_').trim_end_matches('_');

    if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        let mut prefixed = String::with_capacity(trimmed.len().saturating_add(1));
        prefixed.push('_');
        prefixed.push_str(trimmed);
        return prefixed;
    }

    trimmed.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    mod to_ident_fragment_fn {
        use super::*;

        #[rstest]
        #[case::empty("", "")]
        #[case::all_non_alphanumeric("---", "")]
        #[case::unicode("caf\u{e9}", "caf")]
        #[case::digits_only("123", "_123")]
        #[case::digits_then_words("123 cases", "_123_cases")]
        #[case::single_char("a", "a")]
        #[case::mixed_case_with_dashes("CamelCase_With-Dashes", "camelcase_with_dashes")]
        fn sanitizes_to_expected_fragment(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(to_ident_fragment(input), expected);
        }
    }
}
