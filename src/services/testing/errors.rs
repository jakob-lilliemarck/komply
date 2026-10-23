use std::path::PathBuf;

use crate::services::testing::repositories::rules;

/// Errors from resolving and preparing a test.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("target `{target}` is outside the working directory `{root}`")]
    TargetOutsideRoot { root: PathBuf, target: PathBuf },
    #[error("rule file `{0}` has no `path`; `path` is required unless `--rules` selects the file")]
    MissingRulePath(PathBuf),
    #[error("invalid glob `{pattern}` in `{path}`: {source}")]
    InvalidRulePath {
        path: PathBuf,
        pattern: String,
        source: globset::Error,
    },
    #[error("no rules match `{target}`; available paths: {available:?}")]
    NoRulesMatch {
        target: String,
        available: Vec<String>,
    },
    #[error("no enabled rules match `{target}`")]
    NoEnabledRules { target: String },
    #[error("duplicate rule id `{id}` in `{first}` and `{second}`")]
    DuplicateRuleId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
    #[error("no files to check under `{0}`")]
    NoFiles(PathBuf),
    #[error(
        "document is {bytes} bytes, over the {limit}-byte limit; raise `max_document_bytes` or narrow the target"
    )]
    DocumentTooLarge { bytes: usize, limit: usize },
    #[error(transparent)]
    Rules(#[from] rules::Error),
    #[error(transparent)]
    Files(#[from] crate::services::testing::repositories::files::Error),
    #[error(transparent)]
    Decisions(#[from] crate::services::testing::repositories::decisions::Error),
}
