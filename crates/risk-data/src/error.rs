//! The crate's error type — see [`RiskDataError`].

use crate::types::Platform;

use regex::Error as RegexError;
use toml::de::Error as TomlError;

use std::fmt;
use std::io;

/// Everything that can go wrong in [`crate::generate_command_table`]: a directory entry that
/// couldn't be read, a malformed TOML file, or a cross-reference the data itself violates.
#[derive(Debug, thiserror::Error)]
pub enum RiskDataError {
    /// The root passed to [`crate::generate_command_table`] doesn't exist, or exists but isn't
    /// a directory.
    #[error("{path}: not a directory")]
    MissingRoot {
        /// The path that failed the check.
        path: Box<str>,
    },
    /// A directory entry under the scanned root couldn't be read (opening the directory,
    /// reading an entry, or reading a `.toml` file's content).
    #[error("{path}: {source}")]
    Io {
        /// The path that failed.
        path: Box<str>,
        /// The underlying I/O failure.
        #[source]
        source: Box<io::Error>,
    },
    /// The scanned root exists and is a directory, but contains no `.toml` file, recursively, to
    /// parse.
    #[error("{path}: no .toml files found")]
    NoTomlFiles {
        /// The directory that was scanned.
        path: Box<str>,
    },
    /// `file`'s content isn't valid TOML, or doesn't match the schema.
    #[error("{file}: {source}")]
    Toml {
        /// The file that failed to parse.
        file: Box<str>,
        /// The underlying TOML deserialization failure.
        #[source]
        source: Box<TomlError>,
    },
    /// Two commands (or a command and another command's alias) share the same name under the
    /// same [`crate::Platform`].
    #[error("duplicate command name or alias under platform {platform}: {name}")]
    DuplicateCommand {
        /// The name claimed more than once.
        name: Box<str>,
        /// The platform both declarations share.
        platform: Platform,
    },
    /// One command (or one of its subcommands) declares the same flag spelling twice.
    #[error("command {command}{}: duplicate flag spelling {spelling}", DisplaySubcommand(subcommand.clone()))]
    DuplicateFlagSpelling {
        /// The command declaring the spelling twice.
        command: Box<str>,
        /// The subcommand whose own flags declared the spelling twice, or `None` for a
        /// global-scope duplicate.
        subcommand: Option<Box<str>>,
        /// The repeated spelling.
        spelling: Box<str>,
    },
    /// A combo rule's `requires` names a flag spelling neither the command nor (when the combo
    /// rule belongs to a subcommand) that subcommand declares.
    #[error("command {command}{}: combo rule requires undeclared flag {spelling}", DisplaySubcommand(subcommand.clone()))]
    UnknownComboFlag {
        /// The command whose combo rule is malformed.
        command: Box<str>,
        /// The subcommand owning the malformed combo rule, or `None` for a global-scope rule.
        subcommand: Option<Box<str>>,
        /// The undeclared spelling it required.
        spelling: Box<str>,
    },
    /// A `value_rule`/`operand_rule` pattern isn't a valid regex.
    #[error("command {command}{}: invalid pattern {pattern}: {source}", DisplaySubcommand(subcommand.clone()))]
    InvalidPattern {
        /// The command declaring the malformed pattern.
        command: Box<str>,
        /// The subcommand declaring the malformed pattern, or `None` for a global-scope rule.
        subcommand: Option<Box<str>>,
        /// The pattern text that failed to compile.
        pattern: Box<str>,
        /// The underlying regex compilation failure.
        #[source]
        source: Box<RegexError>,
    },
    /// Two subcommands of the same command (by name or alias) share the same name.
    #[error("command {command}: duplicate subcommand name or alias: {name}")]
    DuplicateSubcommand {
        /// The command whose subcommands collide.
        command: Box<str>,
        /// The subcommand name claimed more than once.
        name: Box<str>,
    },
    /// A command declares `grammar = "go"` together with `short_flags_combinable = true` — Go's
    /// flag grammar never clusters short flags, so the combination is a contradiction.
    #[error(
        "command {command}: short_flags_combinable is meaningless under the Go flag grammar, which never clusters"
    )]
    CombinableUnderGoGrammar {
        /// The command declaring the contradictory combination.
        command: Box<str>,
    },
}

/// `" in subcommand {0}"` when `Some`, or nothing — the `{}` fragment every
/// [`RiskDataError`] variant with a `subcommand: Option<Box<str>>` field interpolates into its
/// `#[error(...)]` message.
struct DisplaySubcommand(Option<Box<str>>);

impl fmt::Display for DisplaySubcommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .as_ref()
            .map_or(Ok(()), |name| write!(f, " in subcommand {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    mod duplicate_command {
        use super::*;

        #[rstest]
        fn message_names_the_colliding_platform() {
            let error = RiskDataError::DuplicateCommand {
                name: "kill".into(),
                platform: Platform::Bsd,
            };

            assert_eq!(
                error.to_string(),
                "duplicate command name or alias under platform bsd: kill"
            );
        }
    }

    mod display_subcommand {
        use super::*;

        #[rstest]
        fn renders_nothing_when_none() {
            assert_eq!(DisplaySubcommand(None).to_string(), "");
        }

        #[rstest]
        fn renders_the_subcommand_name_when_some() {
            assert_eq!(
                DisplaySubcommand(Some("push".into())).to_string(),
                " in subcommand push"
            );
        }
    }
}
