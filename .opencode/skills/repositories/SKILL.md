---
name: repositories
description: |
  Use when reading, writing, creating, refactoring, or extending repository
  layer code in komply. Covers pure data access, file and HTTP repositories,
  wire models, errors, and tests. Do NOT use for service, config, or CLI logic.
---

# Repository Layer

A repository is a pure data-access boundary. It reads one resource and returns
values. It contains no business logic and makes no decisions about how its data
is used.

Komply has three repositories, each owning one resource:

| Repository | Resource | Side effects |
|---|---|---|
| `rules` | rule TOML files | reads files |
| `files` | the tested directory and `.complyignore` | reads files |
| `decisions` | the OpenRouter Decisions API | one HTTP POST |

All three are read-only. Nothing is written to disk.

## What a repository is not

- It does not decide which rule files apply to a path.
- It does not decide which files are sent to Jev.
- It does not interpret probabilities, thresholds, or verdicts.
- It does not read `current_dir`, environment variables, or the clock unless
  that value is injected by the caller.
- It does not print.
- It does not know about the CLI.

A repository may validate its own resource: TOML shape, required fields,
duplicate ids within a file, a response's required fields. That is data
integrity, not domain policy.

## File structure

```
src/services/testing/repositories/<resource>/
  mod.rs
  repository.rs
  models.rs
```

- `<resource>` is a plural noun (`rules`, `files`, `decisions`).
- `mod.rs` declares the modules and re-exports the public repository, models,
  and error type.
- `repository.rs` holds the repository struct, its constructor, and its methods.
- `models.rs` holds the wire and value types plus their conversion to and from
  the domain shape. Keep wire types private to the module.

## Models

- Keep serialization types private. The service should see domain values, not
  `serde` structs or raw JSON.
- A model that another layer uses belongs in the service's `model.rs`; a model
  used only by the repository stays in the repository's `models.rs`.
- Do not expose fields that only the repository needs.
- A missing optional field is `Option`, not a sentinel value.

## Repository struct and constructor

A repository struct holds only what its resource needs:

```rust
pub struct DecisionsRepository {
    api_key: String,
    base_url: String,
    timeout: Duration,
    retry: RetryPolicy,
    agent: ureq::Agent,
}
```

- Take dependencies as constructor arguments. Do not read globals.
- A constructor that reads the environment is a separate adapter
  (`from_env`); the core constructor (`new`) takes values. Tests use `new`.
- Keep the struct cheap to construct and free of lazy state.

## Errors

Every repository owns a `thiserror` error enum. Include the path, status, or key
that failed. Do not return `String` errors and do not panic on expected
failures.

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read `{path}`: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("could not parse `{path}`: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
}
```

The service converts these with `#[from]`. A repository error should make sense
on its own without the service context.

## File repositories

- Take every path and pattern from the caller. Do not hardcode `.architecture`,
  `.git`, `target`, or a file extension.
- Return paths relative to the base the caller named, with `/` separators in
  any user-facing string.
- Sort the result so the document is deterministic.
- Skip a non-UTF-8 file and return it in a `skipped` list; do not fail the whole
  read for one binary file.
- Prune a directory as soon as it matches an exclude; do not read its children.
- Do not follow symbolic links.
- Apply a caller-supplied size limit before reading a file's bytes.

## HTTP repositories

- The repository owns the wire contract: endpoint, headers, body shape, status
  handling, retries, timeout.
- It does not own the API key's source. `from_env` is the only environment
  reader; `new` takes the key.
- It does not know rule ids, thresholds, verdicts, or what the caller does with
  the answers.
- Validate the response: required keys, expected type, and value range.
- Retry only what the contract says is transient. Return the status and body
  for a non-retryable failure.
- Use a caller-supplied timeout and retry policy so tests can run without
  sleeping.
- Never log the key or put it in the body.

## Tests

- File repositories: use a fixture tree under `tests/fixtures/` and assert
  exact relative paths, ordering, skips, and byte counts.
- HTTP repositories: use a local `TcpListener` stub for the happy path, retry
  path, and each error path. Never call the real network.
- Test the wire mapping separately from the transport: construct a response
  value and assert the mapped domain value.
- Test each public method: one success case and each error it owns.
- No arbitrary sleeps. Use a zero-delay retry policy.
- Assert exact results.

## Naming

- Repository struct: `<Resource>Repository` (`RulesRepository`).
- Resource directory: plural noun (`rules`, `files`, `decisions`).
- Methods: verbs from the data's point of view (`load`, `collect`, `evaluate`).
- Error variants: what failed (`Read`, `Parse`, `Status`, `MissingAnswer`).

## Related skills

- `@services` — who calls repositories and what stays out of them.
- `@config` — how credentials and limits reach a repository.
- `@documentation` — doc comments and plain language.
