use std::path::{Component, Path, PathBuf};

use crate::services::testing::repositories::rules::FileSelection;

/// File patterns from `.complyignore` and the selected rule files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ignore {
    pub project: Vec<String>,
    pub target: Vec<String>,
}

/// One UTF-8 file included in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub relative_path: PathBuf,
    pub contents: String,
}

impl SourceFile {
    /// Returns the target-relative path with `/` separators.
    pub fn display_path(&self) -> String {
        display_path(&self.relative_path)
    }
}

/// One file omitted from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub relative_path: PathBuf,
    pub reason: SkipReason,
}

impl SkippedFile {
    /// Returns the target-relative path with `/` separators.
    pub fn display_path(&self) -> String {
        display_path(&self.relative_path)
    }
}

/// Why a file was omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NonUtf8,
}

/// The files read from a target directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Collected {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<SkippedFile>,
    pub total_bytes: usize,
}

/// Options for collecting files from a target directory.
pub struct CollectOptions<'a> {
    pub selection: &'a FileSelection,
    pub ignore: &'a Ignore,
    pub max_file_bytes: usize,
}

/// Renders a relative path with `/` separators. An empty path renders as `.`.
pub fn display_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => parts.push("..".to_owned()),
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_renders_an_empty_path_as_a_dot() {
        assert_eq!(display_path(Path::new("")), ".");
    }

    #[test]
    fn it_renders_segments_with_forward_slashes() {
        assert_eq!(
            display_path(Path::new("src/services/alpha/service.rs")),
            "src/services/alpha/service.rs"
        );
    }
}
