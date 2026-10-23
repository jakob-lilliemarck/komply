use std::path::PathBuf;

/// Errors from loading rule files.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read `{path}`: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse `{path}`: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("no rule files in `{0}`")]
    NoRuleFiles(PathBuf),
    #[error("rule `{id}` in `{path}` has an empty id")]
    EmptyId { path: PathBuf, id: String },
    #[error("rule `{id}` in `{path}` has an empty statement")]
    EmptyStatement { path: PathBuf, id: String },
    #[error("rule id `{id}` appears more than once in `{path}`")]
    DuplicateId { path: PathBuf, id: String },
    #[error("rule file `{path}` has an empty `path`")]
    EmptyPath { path: PathBuf },
}
