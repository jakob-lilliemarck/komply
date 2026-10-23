---
name: config
description: |
  Use when adding, changing, or reading configuration or credentials in komply.
  Covers the config file, defaults, precedence, validation, and how credentials
  reach the decisions repository. Do NOT use for service or repository logic.
---

# Configuration

Komply has two kinds of outside value:

- **Config** — project policy: model, thresholds, size cap, timeout.
- **Credentials** — the OpenRouter API key and the optional endpoint override.

They enter through different paths.

## Config sources

Precedence, lowest to highest:

1. `Config::default()`.
2. `<root>/.architecture/config.toml`.
3. The CLI's `--model` override.

There is no environment-only config, no `fx-config`, and no DI container. A
missing config file is not an error: it means defaults. A malformed file or an
invalid value is an error.

`Config` is a plain value passed to the service. The service never loads the
file itself and never reads the environment.

## File format

```toml
[comply]
model = "typesafe/jev-1.13"
violation_threshold = 0.80
clean_threshold = 0.20
max_document_bytes = 400000
request_timeout_seconds = 60
```

| Key | Default |
|---|---|
| `model` | `typesafe/jev-1.13` |
| `violation_threshold` | `0.80` |
| `clean_threshold` | `0.20` |
| `max_document_bytes` | `400000` |
| `request_timeout_seconds` | `60` |

## Validation

Validate once, when the file is loaded:

- `clean_threshold < violation_threshold`.
- Both thresholds in `[0, 1]`.
- `max_document_bytes > 0`.
- `request_timeout_seconds > 0`.

Return a `ConfigError::Invalid` naming the key and the bad value. Do not defer
validation to the service.

## Credentials

- `OPENROUTER_API_KEY` is read only by `DecisionsRepository::from_env`, at the
  moment a live run constructs the client.
- `COMPLY_DECISIONS_URL` overrides the endpoint for development and tests. It is
  also read only by `from_env`; absent means the production URL.
- The key is never part of `Config`, never passed to the service, never printed,
  and never written to disk.
- `--dry-run` and every test must work with no key.

## Config is a value

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub model: String,
    pub violation_threshold: f64,
    pub clean_threshold: f64,
    pub max_document_bytes: usize,
    pub request_timeout: std::time::Duration,
}
```

The config file stores seconds; the `Config` value stores a `Duration`. Do the
conversion in `Config::load`.

## Tests

- Missing file returns defaults.
- A complete file returns every value.
- A file with absent keys uses the defaults for those keys.
- Malformed TOML is a parse error.
- Each validation rule has a test.
- Credential tests never assert against a real key; use a lookup closure or a
  missing-key case.

## Related skills

- `@services` — config arrives as a value.
- `@repositories` — the decisions repository owns the key and endpoint.
- `@documentation` — document defaults and validation in plain words.
