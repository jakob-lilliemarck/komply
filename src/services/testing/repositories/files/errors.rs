use std::path::PathBuf;

/// Errors from reading `.complyignore` or collecting files.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read `{path}`: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not walk `{path}`: {source}")]
    Walk {
        path: PathBuf,
        source: walkdir::Error,
    },
    #[error("invalid glob `{pattern}`: {source}")]
    Glob {
        pattern: String,
        source: globset::Error,
    },
    #[error("target is not a directory: `{0}`")]
    NotDirectory(PathBuf),
    #[error("`{path}` is {bytes} bytes, over the {limit}-byte limit")]
    FileTooLarge {
        path: PathBuf,
        bytes: u64,
        limit: usize,
    },
}
