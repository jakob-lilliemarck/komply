use std::fs;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use super::errors::Error;
use super::models::{
    CollectOptions, Collected, Ignore, SkipReason, SkippedFile, SourceFile, display_path,
};
use crate::services::testing::repositories::rules::FileSelection;

/// Reads `.complyignore` and the files under a target directory.
pub struct FilesRepository;

impl FilesRepository {
    /// Creates a files repository.
    pub fn new() -> Self {
        Self
    }

    /// Reads `.complyignore` at `path`.
    ///
    /// A missing file returns an empty [`Ignore`]. Blank lines and lines
    /// starting with `#` are skipped; one trailing `/` is removed from every
    /// other line.
    pub fn load_ignore(&self, path: &Path) -> Result<Ignore, Error> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Ignore::default());
            }
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };

        let mut project = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let pattern = line.strip_suffix('/').unwrap_or(line);
            if pattern.is_empty() {
                continue;
            }
            project.push(pattern.to_owned());
        }

        Ok(Ignore {
            project,
            target: Vec::new(),
        })
    }

    /// Collects the files under `target` that pass the selection and ignores.
    ///
    /// The target must be a directory. Symbolic links are skipped. Files that
    /// are not valid UTF-8 are returned in `skipped` and omitted from `files`.
    /// A file larger than `max_file_bytes` is an error before its bytes are
    /// read. The returned files and skipped files are sorted by relative path.
    pub fn collect(
        &self,
        root: &Path,
        target: &Path,
        options: &CollectOptions<'_>,
    ) -> Result<Collected, Error> {
        let metadata = fs::metadata(target).map_err(|source| Error::Read {
            path: target.to_path_buf(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(Error::NotDirectory(target.to_path_buf()));
        }

        let includes = build_globset(&include_patterns(options.selection))?;
        let mut target_excludes = options.ignore.target.clone();
        for pattern in &options.selection.exclude {
            if !target_excludes.contains(pattern) {
                target_excludes.push(pattern.clone());
            }
        }
        let excludes = build_globset(&target_excludes)?;
        let project = build_globset(&options.ignore.project)?;

        let target_from_root = target.strip_prefix(root).unwrap_or(target);
        for ancestor in target_from_root.ancestors() {
            if project.is_match(display_path(ancestor)) {
                return Ok(Collected::default());
            }
        }

        let mut files = Vec::new();
        let mut skipped = Vec::new();
        let mut total_bytes = 0;

        let mut walker = WalkDir::new(target).follow_links(false).into_iter();
        while let Some(entry) = walker.next() {
            let entry = entry.map_err(|source| {
                let path = source
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| target.to_path_buf());
                Error::Walk { path, source }
            })?;

            let file_type = entry.file_type();
            if file_type.is_symlink() {
                continue;
            }

            let project_relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let target_relative = entry.path().strip_prefix(target).unwrap_or(entry.path());
            let project_match = project.is_match(display_path(project_relative));
            let target_match = excludes.is_match(display_path(target_relative));

            if file_type.is_dir() {
                if entry.depth() == 0 {
                    continue;
                }
                if project_match || target_match {
                    walker.skip_current_dir();
                }
                continue;
            }

            if !file_type.is_file() {
                continue;
            }
            if project_match || target_match {
                continue;
            }
            if !includes.is_match(display_path(target_relative)) {
                continue;
            }

            let metadata = entry.metadata().map_err(|source| Error::Walk {
                path: entry.path().to_path_buf(),
                source,
            })?;
            if metadata.len() > options.max_file_bytes as u64 {
                return Err(Error::FileTooLarge {
                    path: target_relative.to_path_buf(),
                    bytes: metadata.len(),
                    limit: options.max_file_bytes,
                });
            }

            let bytes = fs::read(entry.path()).map_err(|source| Error::Read {
                path: entry.path().to_path_buf(),
                source,
            })?;

            match String::from_utf8(bytes) {
                Ok(contents) => {
                    total_bytes += contents.len();
                    files.push(SourceFile {
                        relative_path: target_relative.to_path_buf(),
                        contents,
                    });
                }
                Err(_) => skipped.push(SkippedFile {
                    relative_path: target_relative.to_path_buf(),
                    reason: SkipReason::NonUtf8,
                }),
            }
        }

        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        skipped.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        Ok(Collected {
            files,
            skipped,
            total_bytes,
        })
    }
}

impl Default for FilesRepository {
    fn default() -> Self {
        Self::new()
    }
}

fn include_patterns(selection: &FileSelection) -> Vec<String> {
    if selection.include.is_empty() {
        vec!["**/*".to_owned()]
    } else {
        selection.include.clone()
    }
}

fn build_globset(patterns: &[String]) -> Result<GlobSet, Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|source| Error::Glob {
            pattern: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|source| Error::Glob {
        pattern: patterns.join(", "),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::*;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn write_bytes(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn selection(include: &[&str], exclude: &[&str]) -> FileSelection {
        FileSelection {
            include: include.iter().map(|value| (*value).to_owned()).collect(),
            exclude: exclude.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn ignore(project: &[&str], target: &[&str]) -> Ignore {
        Ignore {
            project: project.iter().map(|value| (*value).to_owned()).collect(),
            target: target.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn collect(
        root: &Path,
        target: &Path,
        selection: &FileSelection,
        ignore: &Ignore,
        max_file_bytes: usize,
    ) -> Result<Collected, Error> {
        FilesRepository::new().collect(
            root,
            target,
            &CollectOptions {
                selection,
                ignore,
                max_file_bytes,
            },
        )
    }

    fn relative_paths(files: &[SourceFile]) -> Vec<PathBuf> {
        files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect()
    }

    #[test]
    fn it_returns_an_empty_ignore_when_the_file_is_missing() {
        let root = TempDir::new().unwrap();

        let ignore = FilesRepository::new()
            .load_ignore(&root.path().join(".complyignore"))
            .unwrap();

        assert_eq!(ignore, Ignore::default());
    }

    #[test]
    fn it_returns_a_read_error_when_complyignore_is_a_directory() {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join(".complyignore")).unwrap();

        let error = FilesRepository::new()
            .load_ignore(&root.path().join(".complyignore"))
            .unwrap_err();

        assert!(matches!(error, Error::Read { .. }));
    }

    #[test]
    fn it_parses_complyignore() {
        let root = TempDir::new().unwrap();
        write(
            root.path(),
            ".complyignore",
            "# comment\n\nignored/\n*.log\n  spaced  \n/\n",
        );

        let ignore = FilesRepository::new()
            .load_ignore(&root.path().join(".complyignore"))
            .unwrap();

        assert_eq!(
            ignore.project,
            vec![
                "ignored".to_owned(),
                "*.log".to_owned(),
                "spaced".to_owned()
            ]
        );
    }

    #[test]
    fn it_includes_all_utf8_files_by_default() {
        let root = TempDir::new().unwrap();
        write(root.path(), "a.rs", "abc");
        write(root.path(), "nested/b.rs", "de");

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("a.rs"), PathBuf::from("nested/b.rs")]
        );
        assert_eq!(collected.total_bytes, 5);
        assert!(collected.skipped.is_empty());
    }

    #[test]
    fn it_honours_a_target_relative_include() {
        let root = TempDir::new().unwrap();
        write(root.path(), "a.rs", "a");
        write(root.path(), "nested/b.rs", "b");

        let collected = collect(
            root.path(),
            root.path(),
            &selection(&["nested/*.rs"], &[]),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("nested/b.rs")]
        );
    }

    #[test]
    fn it_honours_a_target_relative_exclude_and_prunes() {
        let root = TempDir::new().unwrap();
        write(root.path(), "keep.rs", "k");
        write(root.path(), "skip/a.rs", "a");

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &ignore(&[], &["skip"]),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("keep.rs")]
        );
        assert!(collected.skipped.is_empty());
    }

    #[test]
    fn it_honours_an_exclude_from_the_selection() {
        let root = TempDir::new().unwrap();
        write(root.path(), "keep.rs", "k");
        write(root.path(), "skip/a.rs", "a");

        let collected = collect(
            root.path(),
            root.path(),
            &selection(&[], &["skip"]),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("keep.rs")]
        );
    }

    #[test]
    fn it_honours_a_project_relative_ignore_for_a_nested_path() {
        let root = TempDir::new().unwrap();
        write(root.path(), "keep.rs", "k");
        write(root.path(), "ignored/a.rs", "a");

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &ignore(&["ignored"], &[]),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("keep.rs")]
        );
        assert!(collected.skipped.is_empty());
    }

    #[test]
    fn it_applies_project_and_target_ignores_with_their_own_bases() {
        let root = TempDir::new().unwrap();
        write(root.path(), "src/keep.rs", "k");
        write(root.path(), "src/project_skip/a.rs", "a");
        write(root.path(), "src/target_skip/b.rs", "b");

        let collected = collect(
            root.path(),
            &root.path().join("src"),
            &FileSelection::default(),
            &ignore(&["src/project_skip"], &["target_skip"]),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("keep.rs")]
        );
    }

    #[test]
    fn it_prunes_a_target_inside_an_ignored_directory() {
        let root = TempDir::new().unwrap();
        write(root.path(), "ignored/sub/a.rs", "a");

        let collected = collect(
            root.path(),
            &root.path().join("ignored/sub"),
            &FileSelection::default(),
            &ignore(&["ignored"], &[]),
            100,
        )
        .unwrap();

        assert!(collected.files.is_empty());
        assert!(collected.skipped.is_empty());
    }

    #[test]
    fn it_records_a_non_utf8_file_and_omits_it() {
        let root = TempDir::new().unwrap();
        write(root.path(), "good.rs", "good");
        write_bytes(root.path(), "bad.bin", &[0xff, 0xfe, 0x00]);

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("good.rs")]
        );
        assert_eq!(collected.skipped.len(), 1);
        assert_eq!(collected.skipped[0].relative_path, PathBuf::from("bad.bin"));
        assert_eq!(collected.skipped[0].reason, SkipReason::NonUtf8);
        assert_eq!(collected.total_bytes, 4);
    }

    #[test]
    fn it_returns_files_in_deterministic_path_order() {
        let root = TempDir::new().unwrap();
        write(root.path(), "z.rs", "z");
        write(root.path(), "a.rs", "a");
        write(root.path(), "m/b.rs", "b");

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![
                PathBuf::from("a.rs"),
                PathBuf::from("m/b.rs"),
                PathBuf::from("z.rs")
            ]
        );
    }

    #[test]
    fn it_returns_not_directory_for_a_file_target() {
        let root = TempDir::new().unwrap();
        write(root.path(), "a.rs", "a");

        let error = collect(
            root.path(),
            &root.path().join("a.rs"),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap_err();

        assert!(matches!(error, Error::NotDirectory(_)));
    }

    #[test]
    fn it_returns_a_read_error_for_a_missing_target() {
        let root = TempDir::new().unwrap();

        let error = collect(
            root.path(),
            &root.path().join("missing"),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap_err();

        assert!(matches!(error, Error::Read { .. }));
    }

    #[test]
    fn it_rejects_an_invalid_glob() {
        let root = TempDir::new().unwrap();
        write(root.path(), "a.rs", "a");

        let error = collect(
            root.path(),
            root.path(),
            &selection(&["["], &[]),
            &Ignore::default(),
            100,
        )
        .unwrap_err();

        assert!(matches!(error, Error::Glob { .. }));
    }

    #[test]
    fn it_returns_file_too_large_before_reading() {
        let root = TempDir::new().unwrap();
        write(root.path(), "a.rs", "0123456789");

        let error = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &Ignore::default(),
            5,
        )
        .unwrap_err();

        match error {
            Error::FileTooLarge { path, bytes, limit } => {
                assert_eq!(path, PathBuf::from("a.rs"));
                assert_eq!(bytes, 10);
                assert_eq!(limit, 5);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn it_does_not_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        write(root.path(), "real.rs", "real");
        symlink(root.path().join("real.rs"), root.path().join("link.rs")).unwrap();

        let collected = collect(
            root.path(),
            root.path(),
            &FileSelection::default(),
            &Ignore::default(),
            100,
        )
        .unwrap();

        assert_eq!(
            relative_paths(&collected.files),
            vec![PathBuf::from("real.rs")]
        );
    }
}
