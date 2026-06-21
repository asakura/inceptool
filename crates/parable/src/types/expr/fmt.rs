use super::{Expr, SpecialParam};
use std::fmt;

impl fmt::Display for SpecialParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            Self::AllArgs => "@",
            Self::AllArgsStar => "*",
            Self::ArgCount => "#",
            Self::ExitStatus => "?",
            Self::ShellPid => "$",
            Self::LastBgPid => "!",
            Self::Flags => "-",
        };

        write!(f, "{symbol}")
    }
}

impl Expr<'_> {
    /// Renders this expression's structural fragment, without the enclosing `(word ...)` —
    /// the piece [`fmt::Debug`] wraps once at the top, and [`Self::Interpolated`] reuses
    /// unwrapped for each of its parts so a mixed word reads as one flat node, e.g.
    /// `(interp "pid=" (special "$"))`, rather than nesting a `(word ...)` per part.
    #[expect(
        clippy::use_debug,
        reason = "quoting/escaping text the way Debug-for-str does is exactly the corpus rendering this fn produces, not a debugging remnant"
    )]
    pub(super) fn fmt_fragment(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(s) => write!(f, "{s:?}"),
            Self::VarRef(name) => write!(f, "(var {name:?})"),
            Self::Positional(digits) => write!(f, "(positional {digits:?})"),
            Self::SpecialParam(param) => {
                use fmt::Write as _;

                let mut symbol = String::new();
                write!(symbol, "{param}")?;
                write!(f, "(special {symbol:?})")
            }
            Self::Interpolated(parts) => {
                write!(f, "(interp")?;

                for part in parts {
                    write!(f, " ")?;
                    part.inner.fmt_fragment(f)?;
                }

                write!(f, ")")
            }
        }
    }
}

impl fmt::Debug for Expr<'_> {
    /// Renders every variant as a `(word ...)` node, exposing `VarRef`/`Positional`/
    /// `SpecialParam`/`Interpolated` as their own structural shapes (see [`Self::fmt_fragment`])
    /// rather than collapsing back to reconstructed source text — corpus snapshots pin which
    /// kind of reference a word holds, not just what it reads back as.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(word ")?;
        self.fmt_fragment(f)?;
        write!(f, ")")
    }
}

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(s) => write!(f, "{s}"),
            Expr::VarRef(v) => write!(f, "${v}"),
            Expr::Positional(digits) => write!(f, "${digits}"),
            Expr::SpecialParam(param) => write!(f, "${param}"),
            Expr::Interpolated(parts) => {
                for part in parts {
                    write!(f, "{part}")?;
                }

                Ok(())
            }
        }
    }
}
