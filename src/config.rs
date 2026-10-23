use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// Project policy loaded from `<root>/.architecture/config.toml`.
///
/// A missing file means [`Config::default`]. A malformed file or an invalid
/// value is an error. The value is passed to the service; it is never loaded
/// there.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub model: String,
    pub violation_threshold: f64,
    pub clean_threshold: f64,
    pub max_document_bytes: usize,
    pub request_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "typesafe/jev-1.13".to_owned(),
            violation_threshold: 0.80,
            clean_threshold: 0.20,
            max_document_bytes: 400_000,
            request_timeout: Duration::from_secs(60),
        }
    }
}

impl Config {
    /// Loads the config from `<root>/.architecture/config.toml`.
    ///
    /// Returns the defaults when the file does not exist. Returns an error when
    /// the file cannot be read, cannot be parsed, or contains an invalid value.
    pub fn load(root: &Path) -> Result<Self, ConfigError> {
        let path = root.join(".architecture").join("config.toml");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ConfigError::Read { path, source }),
        };

        let file: ConfigFile = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;

        let mut config = Self::default();
        if let Some(comply) = file.comply {
            if let Some(model) = comply.model {
                config.model = model;
            }
            if let Some(value) = comply.violation_threshold {
                config.violation_threshold = value;
            }
            if let Some(value) = comply.clean_threshold {
                config.clean_threshold = value;
            }
            if let Some(value) = comply.max_document_bytes {
                config.max_document_bytes = value;
            }
            if let Some(value) = comply.request_timeout_seconds {
                config.request_timeout = Duration::from_secs(value);
            }
        }

        config.validate(&path)?;
        Ok(config)
    }

    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        if !(0.0..=1.0).contains(&self.clean_threshold) {
            return Err(invalid(
                path,
                format!(
                    "clean_threshold must be in [0, 1], got {}",
                    self.clean_threshold
                ),
            ));
        }
        if !(0.0..=1.0).contains(&self.violation_threshold) {
            return Err(invalid(
                path,
                format!(
                    "violation_threshold must be in [0, 1], got {}",
                    self.violation_threshold
                ),
            ));
        }
        if self.clean_threshold >= self.violation_threshold {
            return Err(invalid(
                path,
                format!(
                    "clean_threshold ({}) must be less than violation_threshold ({})",
                    self.clean_threshold, self.violation_threshold
                ),
            ));
        }
        if self.max_document_bytes == 0 {
            return Err(invalid(
                path,
                "max_document_bytes must be greater than 0".to_owned(),
            ));
        }
        if self.request_timeout.is_zero() {
            return Err(invalid(
                path,
                "request_timeout_seconds must be greater than 0".to_owned(),
            ));
        }
        Ok(())
    }
}

fn invalid(path: &Path, message: String) -> ConfigError {
    ConfigError::Invalid {
        path: path.to_path_buf(),
        message,
    }
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    comply: Option<ComplySection>,
}

#[derive(Debug, Deserialize)]
struct ComplySection {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    violation_threshold: Option<f64>,
    #[serde(default)]
    clean_threshold: Option<f64>,
    #[serde(default)]
    max_document_bytes: Option<usize>,
    #[serde(default)]
    request_timeout_seconds: Option<u64>,
}

/// Errors from loading project config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
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
    #[error("invalid config in `{path}`: {message}")]
    Invalid { path: PathBuf, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_config(root: &Path, contents: &str) {
        let architecture = root.join(".architecture");
        std::fs::create_dir_all(&architecture).unwrap();
        std::fs::write(architecture.join("config.toml"), contents).unwrap();
    }

    fn complete_config() -> &'static str {
        r#"
[comply]
model = "test/model"
violation_threshold = 0.9
clean_threshold = 0.1
max_document_bytes = 1234
request_timeout_seconds = 7
"#
    }

    #[test]
    fn it_returns_defaults_when_config_is_missing() {
        let root = TempDir::new().unwrap();

        let config = Config::load(root.path()).unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn it_reads_every_value_from_a_complete_file() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), complete_config());

        let config = Config::load(root.path()).unwrap();

        assert_eq!(config.model, "test/model");
        assert_eq!(config.violation_threshold, 0.9);
        assert_eq!(config.clean_threshold, 0.1);
        assert_eq!(config.max_document_bytes, 1234);
        assert_eq!(config.request_timeout, Duration::from_secs(7));
    }

    #[test]
    fn it_uses_defaults_for_keys_absent_from_an_existing_file() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), "[comply]\nmodel = \"test/model\"\n");

        let config = Config::load(root.path()).unwrap();

        assert_eq!(config.model, "test/model");
        assert_eq!(config.violation_threshold, 0.80);
        assert_eq!(config.clean_threshold, 0.20);
        assert_eq!(config.max_document_bytes, 400_000);
        assert_eq!(config.request_timeout, Duration::from_secs(60));
    }

    #[test]
    fn it_returns_a_read_error_when_config_is_a_directory() {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join(".architecture/config.toml")).unwrap();

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Read { .. }));
    }

    #[test]
    fn it_rejects_malformed_toml() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), "[comply\n");

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Parse { .. }));
    }

    #[test]
    fn it_rejects_clean_threshold_at_or_above_violation_threshold() {
        let root = TempDir::new().unwrap();
        write_config(
            root.path(),
            "[comply]\nclean_threshold = 0.8\nviolation_threshold = 0.8\n",
        );

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Invalid { .. }));
    }

    #[test]
    fn it_rejects_thresholds_outside_the_unit_interval() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), "[comply]\nclean_threshold = -0.1\n");

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Invalid { .. }));
    }

    #[test]
    fn it_rejects_zero_max_document_bytes() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), "[comply]\nmax_document_bytes = 0\n");

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Invalid { .. }));
    }

    #[test]
    fn it_rejects_zero_request_timeout() {
        let root = TempDir::new().unwrap();
        write_config(root.path(), "[comply]\nrequest_timeout_seconds = 0\n");

        let error = Config::load(root.path()).unwrap_err();

        assert!(matches!(error, ConfigError::Invalid { .. }));
    }
}
