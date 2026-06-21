use std::borrow::Cow;

use super::Spanned;

mod fmt;

/// One of Bash's punctuation-named special parameters — `$@`, `$*`, `$#`, `$?`, `$$`, `$!`, `$-`.
///
/// These don't carry a name to borrow the way `$NAME`/`$1` do, so they're classified into this
/// enum at the point they're recognized (`parser::word::classify_param`) rather than passed
/// around as a bare symbol string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialParam {
    /// `$@` — positional parameters, each a separate word.
    AllArgs,
    /// `$*` — positional parameters as a single word.
    AllArgsStar,
    /// `$#` — number of positional parameters.
    ArgCount,
    /// `$?` — exit status of the last command.
    ExitStatus,
    /// `$$` — the shell's own PID.
    ShellPid,
    /// `$!` — PID of the last backgrounded command.
    LastBgPid,
    /// `$-` — current shell option flags.
    Flags,
}

/// A parsed word, with any `$NAME`/`${NAME}` references resolved out of the surrounding literal
/// text — see `parser::word`.
#[derive(Clone, PartialEq, Eq)]
pub enum Expr<'a> {
    /// Plain text, not subject to further expansion.
    Literal(Cow<'a, str>),
    /// A `$NAME`/`${NAME}` reference, not yet resolved to a value.
    VarRef(&'a str),
    /// A `$1`/`$12`/`${10}` positional parameter reference, holding the digit text.
    Positional(&'a str),
    /// A `$@`/`$*`/`$#`/`$?`/`$$`/`$!`/`$-` special parameter reference.
    SpecialParam(SpecialParam),
    /// A word containing one or more `$NAME`/`${NAME}` references mixed with literal text,
    /// e.g. `"prefix${x}suffix"` is `[Literal("prefix"), VarRef("x"), Literal("suffix")]`. Each
    /// part carries its own byte span (set once in `parser::word::interpolate`), rather than the
    /// whole word's span being reused for every part.
    Interpolated(Vec<Spanned<Self>>),
}
