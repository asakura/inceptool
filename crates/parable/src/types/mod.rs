//! The AST and token types shared by [`crate::lexer`], [`crate::parser`], and downstream
//! analysis — see [`Token`], [`Expr`], and [`Statement`].

pub use expr::{Expr, SpecialParam};
pub use lexer::LexerState;
pub(crate) use redirect::strip_delimiter_quoting;
pub use redirect::{Redirect, RedirectKind, RedirectTarget};
pub use spanned::Spanned;
pub use statement::{CaseArm, LogicalOp, PipeOp, Statement};
pub use token::Token;

mod expr;
mod lexer;
mod redirect;
mod spanned;
mod statement;
mod token;
