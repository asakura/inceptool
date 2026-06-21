use std::fmt;
use std::ops::Range;

/// A wrapper attaching a byte span (start and end offset) to an AST node or token.
///
/// If the `miette` feature is enabled, this type implements conversions to
/// [`miette::SourceSpan`], allowing seamless integration with `miette` diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The inner parsed node.
    pub inner: T,
    /// The byte span in the source string.
    pub span: Range<usize>,
}

impl<T> From<(T, Range<usize>)> for Spanned<T> {
    /// Builds a `Spanned` from a `(node, span)` tuple — the shape winnow's `.with_span()`
    /// combinator produces, so a parser can finish with `.with_span().map(Spanned::from)`.
    fn from((inner, span): (T, Range<usize>)) -> Self {
        Self { inner, span }
    }
}

/// Delegates `$trait` straight through to a `Spanned<T>`'s `inner`, so a `Spanned` reads exactly
/// like the node it wraps. In particular, this keeps `{:?}` corpus-test AST snapshots from being
/// polluted with spans, without [`fmt::Debug`] and [`fmt::Display`] each needing their own
/// hand-written pass-through.
macro_rules! transparent_fmt {
    ($trait:ident) => {
        impl<T: fmt::$trait> fmt::$trait for Spanned<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::$trait::fmt(&self.inner, f)
            }
        }
    };
}

transparent_fmt!(Display);
transparent_fmt!(Debug);

#[cfg(feature = "miette")]
impl<T> From<Spanned<T>> for miette::SourceSpan {
    fn from(spanned: Spanned<T>) -> Self {
        Self::from(spanned.span)
    }
}

#[cfg(feature = "miette")]
impl<T> From<&Spanned<T>> for miette::SourceSpan {
    /// Delegates to the owned [`From<Spanned<T>>`] impl above via a `start..end` reconstructed
    /// from copies of `span`'s two `usize` fields, rather than `Range::clone`-ing the whole span
    /// — cheaper, and `T` need not be `Clone` for it (`Spanned<T>` itself can't be reused here
    /// without one).
    fn from(spanned: &Spanned<T>) -> Self {
        Self::from(spanned.span.start..spanned.span.end)
    }
}
