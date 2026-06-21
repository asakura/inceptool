//! Build script for the inceptool-parable crate.
//!
//! # Build Script Architecture
//!
//! Reads `.tests` files from the `corpus/` directory, parses them via
//! [`inceptool_corpus_parser`], and generates `rstest`-based integration
//! test functions into `OUT_DIR/generated_tests.rs`.
//!
//! ## Core Design
//!
//! Parsing is delegated to the [`inceptool_corpus_parser`] crate; this
//! script is responsible only for filesystem discovery, code generation
//! via the `quote` crate, and writing the output file.
//!
//! A case's `expected` block is ordinary AST text, with one reserved value:
//! [`ERROR_EXPECTED`] (`<error>`) marks the case as negative — the input must
//! fail to parse, rather than produce that literal AST. This is how the
//! corpus expresses "this is invalid Bash, and must be rejected" rather than
//! "this is invalid Bash, but the parser doesn't notice and produces some AST
//! anyway" (a silent-misparse bug, not a passing test).
//!
//! ## Flow
//!
//! 1. Iterate over `.tests` files in `corpus/`.
//! 2. Parse each file into a [`TestSuite`].
//! 3. Validate that suite numbers are unique.
//! 4. Partition each group's cases into positive (expect a specific AST) and
//!    negative ([`ERROR_EXPECTED`]) cases, and generate Rust test functions
//!    for whichever of the two are present.
//! 5. Write all generated functions to a single file in `OUT_DIR`.
//!
//! ## Edge Cases
//!
//! - Missing `corpus/` directory: emits an empty test file and exits.
//! - Duplicate suite numbers across files: errors out.
//! - A group made entirely of negative cases gets only a `fails_to_parse` fn
//!   (no `parses`/`roundtrips`, since there's no AST to compare or round-trip).

use inceptool_corpus_parser::{CorpusCase, CorpusParseError, TestSuite, to_ident_fragment};

use quote::{format_ident, quote};

use std::collections::BTreeSet;
use std::path::Path;
use std::{env, fmt, fs, io};

/// Directory containing the corpus `.tests` files.
const CORPUS_DIR: &str = "corpus";

/// Name of the generated test file written to `OUT_DIR`.
const GENERATED_FILE_NAME: &str = "generated_tests.rs";

/// Reserved `expected` value marking a case as negative: `input` must fail to parse, rather
/// than produce this text as its rendered AST.
const ERROR_EXPECTED: &str = "<error>";

/// Errors that can occur during the build script execution.
#[derive(thiserror::Error)]
enum BuildError {
    /// A corpus `.tests` file could not be parsed.
    #[error(transparent)]
    CorpusParse(#[from] CorpusParseError),

    /// A filesystem operation failed.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The `OUT_DIR` environment variable is not set.
    #[error("environment variable OUT_DIR is not set")]
    OutDirNotSet,

    /// A file in the corpus directory has an invalid (non-UTF-8) stem.
    #[error("invalid file stem")]
    InvalidFileStem,

    /// Two suite files declare the same suite number.
    #[error("duplicate suite number: {number}")]
    DuplicateSuiteNumber {
        /// The duplicated number.
        number: u32,
    },
}

impl fmt::Debug for BuildError {
    /// Delegates to [`Display`](fmt::Display) so the build error output is
    /// human-readable rather than a raw `Debug` dump.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Generates a test function for parsing assertions over `cases` — the positive subset of a
/// group's cases (see [`ERROR_EXPECTED`]).
#[must_use = "returns the generated test function token stream"]
fn generate_test_fn(cases: &[CorpusCase<'_>]) -> proc_macro2::TokenStream {
    let case_tokens = cases.iter().map(|c| {
        let ident = format_ident!("{}", to_ident_fragment(&c.name));
        let input = &c.input;
        let expected = &c.expected;

        quote! { #[case::#ident(#input, #expected)] }
    });

    quote! {
        #[rstest::rstest]
        #(#case_tokens)*
        fn parses(#[case] input: &str, #[case] expected: &str) -> Result<(), TestError> {
            let actual = inceptool_parable::render_program_ast(input).map_err(|e| {
                TestError::Failure(format!(
                    "Parse error: {}", inceptool_parable::ParseErrorDisplay(&e)
                ))
            })?;

            if actual != expected {
                if expected.is_empty() {
                    return Err(TestError::Failure(format!("Expected empty output, got:\n{actual}")));
                }

                return Err(TestError::Failure(format!(
                    "\nAST mismatch!\nExpected:\n{expected}\nActual:\n{actual}\n"
                )));
            }

            Ok(())
        }
    }
}

/// Generates a test function for round-trip assertions over `cases` — the positive subset of a
/// group's cases (see [`ERROR_EXPECTED`]).
#[must_use = "returns the generated roundtrip test function token stream"]
fn generate_roundtrip_test_fn(cases: &[CorpusCase<'_>]) -> proc_macro2::TokenStream {
    let case_tokens = cases.iter().map(|c| {
        let ident = format_ident!("{}", to_ident_fragment(&c.name));
        let input = &c.input;

        quote! { #[case::#ident(#input)] }
    });

    quote! {
        #[rstest::rstest]
        #(#case_tokens)*
        fn roundtrips(#[case] input: &str) -> Result<(), TestError> {
            let parsed = inceptool_parable::parse_program(input).map_err(|e| {
                TestError::Failure(format!(
                    "Parse error: {}", inceptool_parable::ParseErrorDisplay(&e)
                ))
            })?;

            let rendered = parsed
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");

            let reparsed = inceptool_parable::parse_program(&rendered).map_err(|e| {
                TestError::Failure(format!(
                    "Re-parse error on rendered bash:\n{rendered}\nError: {}",
                    inceptool_parable::ParseErrorDisplay(&e)
                ))
            })?;

            let ast_before = parsed.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join("\n");
            let ast_after = reparsed.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join("\n");

            if ast_before != ast_after {
                return Err(TestError::Failure(format!(
                    "\nRound-trip AST mismatch!\nOriginal input:\n{input}\nRendered bash:\n{rendered}\nOriginal AST:\n{ast_before}\nReparsed AST:\n{ast_after}\n"
                )));
            }

            Ok(())
        }
    }
}

/// Generates a test function asserting that every case in `cases` — the negative subset of a
/// group's cases (see [`ERROR_EXPECTED`]) — fails to parse.
#[must_use = "returns the generated negative-case test function token stream"]
fn generate_error_test_fn(cases: &[CorpusCase<'_>]) -> proc_macro2::TokenStream {
    let case_tokens = cases.iter().map(|c| {
        let ident = format_ident!("{}", to_ident_fragment(&c.name));
        let input = &c.input;

        quote! { #[case::#ident(#input)] }
    });

    quote! {
        #[rstest::rstest]
        #(#case_tokens)*
        fn fails_to_parse(#[case] input: &str) -> Result<(), TestError> {
            if let Ok(parsed) = inceptool_parable::parse_program(input) {
                let ast = parsed.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join("\n");

                return Err(TestError::Failure(format!(
                    "expected a syntax error, but it parsed successfully as:\n{ast}"
                )));
            }

            Ok(())
        }
    }
}

fn main() -> Result<(), BuildError> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={CORPUS_DIR}");

    let out_dir = env::var_os("OUT_DIR").ok_or(BuildError::OutDirNotSet)?;
    let dest_path = Path::new(&out_dir).join(GENERATED_FILE_NAME);
    let tests_dir = Path::new(CORPUS_DIR);

    if !tests_dir.exists() {
        fs::write(&dest_path, "")?;
        return Ok(());
    }

    let mut test_modules = Vec::new();
    let mut seen_suite_numbers = BTreeSet::new();

    for entry_res in fs::read_dir(tests_dir)? {
        let entry = entry_res?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "tests") {
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or(BuildError::InvalidFileStem)?;
            let content = fs::read_to_string(&path)?;
            let suite = TestSuite::parse(file_stem, &content)?;

            if !seen_suite_numbers.insert(suite.number) {
                return Err(BuildError::DuplicateSuiteNumber {
                    number: suite.number,
                });
            }

            let suite_name_clean = to_ident_fragment(&suite.name);
            let suite_mod_name = format_ident!("suite_{}_{}", suite.number, suite_name_clean);
            let mut group_modules = Vec::new();

            for group in &suite.groups {
                let group_name_clean = format_ident!("{}", to_ident_fragment(&group.name));

                let (negative_cases, positive_cases): (Vec<_>, Vec<_>) = group
                    .cases
                    .iter()
                    .cloned()
                    .partition(|c| c.expected.as_ref() == ERROR_EXPECTED);

                let mut group_fns = Vec::new();

                if !positive_cases.is_empty() {
                    group_fns.push(generate_test_fn(&positive_cases));
                    group_fns.push(generate_roundtrip_test_fn(&positive_cases));
                }

                if !negative_cases.is_empty() {
                    group_fns.push(generate_error_test_fn(&negative_cases));
                }

                group_modules.push(quote! {
                    mod #group_name_clean {
                        use super::*;

                        #(#group_fns)*
                    }
                });
            }

            test_modules.push(quote! {
                mod #suite_mod_name {
                    use super::*;

                    #(#group_modules)*
                }
            });
        }
    }

    let tokens = quote! {
        #(#test_modules)*
    };

    fs::write(&dest_path, tokens.to_string())?;

    Ok(())
}
