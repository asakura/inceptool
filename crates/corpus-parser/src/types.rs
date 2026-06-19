//! Data types for corpus `.tests` files.

use std::borrow::Cow;

/// One test case block from a corpus `.tests` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusCase<'a> {
    /// Raw case description as written in the `.tests` file.
    pub name: Cow<'a, str>,
    /// Raw input text for the test case.
    ///
    /// A literal `---` or `===` line in the source `.tests` file is written
    /// escaped (`\---`/`\===`) and already unescaped here.
    pub input: Cow<'a, str>,
    /// Expected output text for the test case.
    ///
    /// A literal `---` or `===` line in the source `.tests` file is written
    /// escaped (`\---`/`\===`) and already unescaped here.
    pub expected: Cow<'a, str>,
}

impl CorpusCase<'_> {
    /// Converts all borrowed fields to owned, producing a `'static` lifetime.
    #[must_use = "returns the owned copy; original is consumed"]
    pub fn into_owned(self) -> CorpusCase<'static> {
        CorpusCase {
            name: Cow::Owned(self.name.into_owned()),
            input: Cow::Owned(self.input.into_owned()),
            expected: Cow::Owned(self.expected.into_owned()),
        }
    }
}

/// A group of test cases within a test suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestGroup<'a> {
    /// Raw group name as written in the `# === Name ===` header.
    pub name: Cow<'a, str>,
    /// The individual test cases belonging to this group.
    pub cases: Vec<CorpusCase<'a>>,
}

impl TestGroup<'_> {
    /// Converts all borrowed fields to owned, producing a `'static` lifetime.
    #[must_use = "returns the owned copy; original is consumed"]
    pub fn into_owned(self) -> TestGroup<'static> {
        TestGroup {
            name: Cow::Owned(self.name.into_owned()),
            cases: self.cases.into_iter().map(CorpusCase::into_owned).collect(),
        }
    }
}

/// A parsed test suite representing a single `.tests` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestSuite<'a> {
    /// The suite's unique numeric identifier.
    pub number: u32,
    /// Raw suite name as declared in the `#! Suite N: Name` header.
    pub name: Cow<'a, str>,
    /// The groups of test cases within this suite.
    pub groups: Vec<TestGroup<'a>>,
}

impl TestSuite<'_> {
    /// Converts all borrowed fields to owned, producing a `'static` lifetime.
    #[must_use = "returns the owned copy; original is consumed"]
    pub fn into_owned(self) -> TestSuite<'static> {
        TestSuite {
            number: self.number,
            name: Cow::Owned(self.name.into_owned()),
            groups: self.groups.into_iter().map(TestGroup::into_owned).collect(),
        }
    }
}
