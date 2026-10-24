use std::path::PathBuf;

use serde::Deserialize;

/// One rule file and the rules it contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub paths: Vec<String>,
    pub files: FileSelection,
    pub rules: Vec<Rule>,
    pub source: PathBuf,
}

/// The file filters carried by one rule file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSelection {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// One declarative statement sent to the decisions model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub statement: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RuleFile {
    #[serde(default)]
    pub(crate) path: Option<PathSpec>,
    #[serde(default)]
    pub(crate) files: Option<FilesSection>,
    #[serde(default)]
    pub(crate) rules: Vec<RuleEntry>,
}

/// A rule file's path selector: one glob or several.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PathSpec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct FilesSection {
    #[serde(default)]
    pub(crate) include: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) exclude: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RuleEntry {
    pub(crate) id: String,
    pub(crate) statement: String,
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
}

fn default_enabled() -> bool {
    true
}
