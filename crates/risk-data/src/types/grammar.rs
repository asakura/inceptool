//! How a command's flag tokens are spelled and tokenized — see [`FlagGrammar`].

use serde::Deserialize;

/// Which flag-syntax family a command follows.
///
/// Tokenization itself (long/short dash conventions, clustering, value-attachment) is runtime
/// classification logic, not schema — see `inceptool_parable::risk`'s grammar-dispatched
/// `flag_tokens_*` functions. This enum only selects which of those rules applies. Every
/// existing data file omits this field and gets [`Self::Gnu`] by default, reproducing today's
/// one hardcoded tokenization scheme exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagGrammar {
    /// `--long[=value]`, `-short`; a run of single-dash lowercase letters clusters into one flag
    /// per letter iff the command's `short_flags_combinable` says so.
    #[default]
    Gnu,
    /// `-short` only — a `--`-prefixed token (other than the bare `--` end-of-options marker) is
    /// never parsed as a flag. Matches traditional BSD/macOS userland getopt conventions.
    Bsd,
    /// `-name` and `--name` are the *same* flag regardless of length, and are never clustered —
    /// Go's `flag` package convention.
    Go,
}
