use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::errors::Error;
use super::models::{FileSelection, Rule, RuleFile, RuleSet};

/// Reads rule files. It loads one file, or every `*.toml` directly inside a
/// directory except `config.toml`.
pub struct RulesRepository;

impl RulesRepository {
    /// Creates a rule repository.
    pub fn new() -> Self {
        Self
    }

    /// Loads the rule sets at `path`.
    ///
    /// A file is loaded exactly. A directory is loaded in lexicographic
    /// filename order, skipping `config.toml` and subdirectories. An empty
    /// directory is an error. The repository validates the file shape, required
    /// fields, and duplicate ids within one file.
    pub fn load(&self, path: &Path) -> Result<Vec<RuleSet>, Error> {
        let metadata = fs::metadata(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;

        if !metadata.is_dir() {
            return Ok(vec![load_file(path)?]);
        }

        let entries = fs::read_dir(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::Read {
                path: path.to_path_buf(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| Error::Read {
                path: path.to_path_buf(),
                source,
            })?;
            if !file_type.is_file() {
                continue;
            }

            let file_path = entry.path();
            if file_path.file_name().and_then(|name| name.to_str()) == Some("config.toml") {
                continue;
            }
            if file_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("toml")
            {
                continue;
            }
            files.push(file_path);
        }

        files.sort();
        if files.is_empty() {
            return Err(Error::NoRuleFiles(path.to_path_buf()));
        }

        files.iter().map(|file| load_file(file)).collect()
    }
}

impl Default for RulesRepository {
    fn default() -> Self {
        Self::new()
    }
}

fn load_file(path: &Path) -> Result<RuleSet, Error> {
    let contents = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let file: RuleFile = toml::from_str(&contents).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    let path_value = match file.path {
        Some(path_value) if path_value.trim().is_empty() => {
            return Err(Error::EmptyPath {
                path: path.to_path_buf(),
            });
        }
        path_value => path_value,
    };

    let files = file
        .files
        .map(|files| FileSelection {
            include: files.include.unwrap_or_default(),
            exclude: files.exclude.unwrap_or_default(),
        })
        .unwrap_or_default();

    let mut seen = HashSet::new();
    let mut rules = Vec::with_capacity(file.rules.len());
    for entry in file.rules {
        let id = entry.id;
        let statement = entry.statement;
        if id.trim().is_empty() {
            return Err(Error::EmptyId {
                path: path.to_path_buf(),
                id,
            });
        }
        if statement.trim().is_empty() {
            return Err(Error::EmptyStatement {
                path: path.to_path_buf(),
                id,
            });
        }
        if !seen.insert(id.clone()) {
            return Err(Error::DuplicateId {
                path: path.to_path_buf(),
                id,
            });
        }
        rules.push(Rule {
            id,
            statement,
            enabled: entry.enabled,
        });
    }

    Ok(RuleSet {
        path: path_value,
        files,
        rules,
        source: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(root: &Path, name: &str, contents: &str) {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join(name), contents).unwrap();
    }

    fn rule_file(path: Option<&str>, id: &str) -> String {
        let path_line = path
            .map(|path| format!("path = \"{path}\"\n"))
            .unwrap_or_default();
        format!("{path_line}\n[[rules]]\nid = \"{id}\"\nstatement = \"The statement.\"\n")
    }

    #[test]
    fn it_loads_a_single_file() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(&file, rule_file(Some("src/*"), "rule.one")).unwrap();
        let repository = RulesRepository::new();

        let sets = repository.load(&file).unwrap();

        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].path.as_deref(), Some("src/*"));
        assert_eq!(sets[0].rules.len(), 1);
        assert_eq!(sets[0].rules[0].id, "rule.one");
        assert_eq!(sets[0].source, file);
    }

    #[test]
    fn it_loads_a_directory_in_filename_order_and_ignores_config_toml() {
        let directory = TempDir::new().unwrap();
        write_file(
            directory.path(),
            "b.toml",
            &rule_file(Some("b/*"), "rule.b"),
        );
        write_file(
            directory.path(),
            "a.toml",
            &rule_file(Some("a/*"), "rule.a"),
        );
        write_file(directory.path(), "config.toml", "not a rule file");
        let repository = RulesRepository::new();

        let sets = repository.load(directory.path()).unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].source.file_name().unwrap(), "a.toml");
        assert_eq!(sets[1].source.file_name().unwrap(), "b.toml");
    }

    #[test]
    fn it_rejects_an_empty_directory() {
        let directory = TempDir::new().unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(directory.path()).unwrap_err();

        assert!(matches!(error, Error::NoRuleFiles(_)));
    }

    #[test]
    fn it_returns_a_read_error_for_a_missing_path() {
        let directory = TempDir::new().unwrap();
        let repository = RulesRepository::new();

        let error = repository
            .load(&directory.path().join("missing"))
            .unwrap_err();

        assert!(matches!(error, Error::Read { .. }));
    }

    #[test]
    fn it_rejects_malformed_toml() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(&file, "[rules\n").unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(&file).unwrap_err();

        assert!(matches!(error, Error::Parse { .. }));
    }

    #[test]
    fn it_rejects_an_empty_id() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(&file, rule_file(Some("src/*"), "")).unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(&file).unwrap_err();

        assert!(matches!(error, Error::EmptyId { .. }));
    }

    #[test]
    fn it_rejects_an_empty_statement() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(
            &file,
            "path = \"src/*\"\n[[rules]]\nid = \"rule.one\"\nstatement = \"  \"\n",
        )
        .unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(&file).unwrap_err();

        assert!(matches!(error, Error::EmptyStatement { .. }));
    }

    #[test]
    fn it_rejects_an_empty_path() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(&file, rule_file(Some("  "), "rule.one")).unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(&file).unwrap_err();

        assert!(matches!(error, Error::EmptyPath { .. }));
    }

    #[test]
    fn it_rejects_a_duplicate_id_within_one_file() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(
            &file,
            "path = \"src/*\"\n[[rules]]\nid = \"rule.one\"\nstatement = \"One.\"\n[[rules]]\nid = \"rule.one\"\nstatement = \"Two.\"\n",
        )
        .unwrap();
        let repository = RulesRepository::new();

        let error = repository.load(&file).unwrap_err();

        assert!(matches!(error, Error::DuplicateId { .. }));
    }

    #[test]
    fn it_defaults_enabled_to_true_and_keeps_an_absent_path_as_none() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(&file, rule_file(None, "rule.one")).unwrap();
        let repository = RulesRepository::new();

        let sets = repository.load(&file).unwrap();

        assert_eq!(sets[0].path, None);
        assert!(sets[0].rules[0].enabled);
    }

    #[test]
    fn it_keeps_file_filters() {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("rules.toml");
        std::fs::write(
            &file,
            "path = \"src/*\"\n[files]\ninclude = [\"**/*\"]\nexclude = [\"target/**\"]\n[[rules]]\nid = \"rule.one\"\nstatement = \"One.\"\n",
        )
        .unwrap();
        let repository = RulesRepository::new();

        let sets = repository.load(&file).unwrap();

        assert_eq!(sets[0].files.include, vec!["**/*".to_owned()]);
        assert_eq!(sets[0].files.exclude, vec!["target/**".to_owned()]);
    }
}
