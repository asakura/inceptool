//! Helper for probing a tool's raw JSON input for the file path it targets.

use serde_json::Value;

/// Field names checked, in order, when probing tool-input JSON for the path
/// of the file a tool operates on. Different drivers populate different
/// fields: Claude Code uses `file_path`, Gemini uses `path`/`AbsolutePath`.
const FILE_PATH_FIELDS: &[&str] = &["file_path", "path", "AbsolutePath"];

/// Returns the first non-empty string among [`FILE_PATH_FIELDS`] present in
/// `input`, checked in order; falls through to the next field when a field
/// is present but empty.
#[must_use = "returns the extracted file path; discarding it loses the lookup"]
pub fn extract_file_path(input: &Value) -> Option<&str> {
    FILE_PATH_FIELDS.iter().find_map(|key| {
        input
            .get(*key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Json(#[from] serde_json::Error),
    }

    mod extract_file_path {
        use super::*;

        #[rstest]
        #[case::file_path_field(r#"{"file_path": "src/main.rs"}"#, Some("src/main.rs"))]
        #[case::path_field(r#"{"path": "src/main.rs"}"#, Some("src/main.rs"))]
        #[case::absolute_path_field(r#"{"AbsolutePath": "src/main.rs"}"#, Some("src/main.rs"))]
        #[case::empty_string_filtered(r#"{"file_path": ""}"#, None)]
        #[case::empty_first_falls_back(
            r#"{"file_path": "", "path": "src/main.rs"}"#,
            Some("src/main.rs")
        )]
        #[case::missing_fields(r#"{"other": "value"}"#, None)]
        fn extraction(#[case] json: &str, #[case] expected: Option<&str>) -> Result<(), TestError> {
            let parsed: Value = serde_json::from_str(json)?;
            assert_eq!(super::extract_file_path(&parsed), expected);
            Ok(())
        }
    }
}
