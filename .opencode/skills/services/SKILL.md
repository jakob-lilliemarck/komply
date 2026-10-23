---
name: services
description: |
  Use when creating, changing, or reviewing a service in komply. Covers what a
  service owns, how it composes repositories, its module layout, errors, and
  tests. Do NOT use for repository, config, or CLI logic.
---

# Services

A service orchestrates business logic. It coordinates repositories and external
resources to carry out one domain operation. It is the only layer that makes
domain decisions.

Komply has one service, `testing`, under `src/services/testing/`. It decides
which rule files apply, which files belong in the document, how the Jev
questions are built, and how probabilities become verdicts.

## What a service is not

A service does not read files, open sockets, read environment variables, read
the current directory, print, or choose an exit code. Those belong to a
repository or to the CLI.

A service may assemble a value for transport — the Jev state document, for
example — but the assembly is a pure transformation over values it was given.

## Public interface conventions

Public methods model business actions, not data operations:

```rust
pub fn resolve_rules(&self, request: &TestRequest) -> Result<Vec<RuleSet>, Error>;
pub fn prepare(&self, request: TestRequest) -> Result<PreparedTest, Error>;
pub fn run(&self, prepared: PreparedTest, decisions: &DecisionsRepository) -> Result<Report, Error>;
pub fn check(&self, request: TestRequest, decisions: &DecisionsRepository) -> Result<Report, Error>;
```

Names are verbs. Inputs and outputs are domain values, not raw file contents or
HTTP bodies.

## Composing dependencies

A service holds concrete repositories and plain values:

- `RulesRepository` — rule TOML.
- `FilesRepository` — source files and `.complyignore`.
- `Config` — thresholds, model, limits.

The decisions client is passed to `run` rather than stored on the service. That
keeps `prepare` and `--dry-run` free of credentials and makes a stub client a
normal argument.

There is no DI container, no `registrations.rs`, and no service-to-service
lookup. The CLI constructs the service and its repositories.

## No assumptions about the surrounding system

- The service never calls `std::env`, `current_dir`, `stdout`, or `stderr`.
- It never opens a file or a socket directly.
- It never prints.
- It never chooses an exit code.
- Paths, config, and credentials arrive as values.
- If the service needs something from the outside, it is a repository method or
  a method argument.

## Module structure

```
src/services/testing/
  mod.rs
  model.rs        # domain values: Rule, RuleSet, Document, Report, Verdict
  service.rs      # Service + pure policy functions
  errors.rs       # one service error enum
  repositories/
    rules/
    files/
    decisions/
```

`mod.rs` declares the modules and re-exports the public service types. Keep
`model.rs` free of IO. Keep `service.rs` free of IO. Pure helpers such as rule
matching, selection merging, document rendering, request building, and verdict
mapping are free functions or methods on the value types, so they can be tested
without a filesystem.

## Errors

One `thiserror` enum in `errors.rs` accumulates what the service's callees can
return:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no rules match `{target}`; available paths: {available:?}")]
    NoRulesMatch { target: String, available: Vec<String> },
    #[error(transparent)]
    Rules(#[from] crate::services::testing::repositories::rules::Error),
    #[error(transparent)]
    Files(#[from] crate::services::testing::repositories::files::Error),
    #[error(transparent)]
    Decisions(#[from] crate::services::testing::repositories::decisions::Error),
}
```

Add variants for service-owned rules (no match, no enabled rules, duplicate ids,
oversized document) and `#[from]` conversions for every repository the service
calls. Do not flatten a repository error into a string; the caller may need the
typed source.

## The service-repository boundary

- The service decides *which* rules apply and *which* files are sent. The
  repositories provide the data and apply the patterns they are handed.
- The service decides thresholds and verdicts. The decisions repository only
  returns probabilities.
- A repository method that only forwards to another repository belongs on the
  repository, not the service.
- A service method that only forwards to one read method is an anti-pattern;
  place the read on the repository and let the caller use it.

## Tests

- Test pure functions directly: matching, merging, document rendering, request
  building, and verdict boundaries.
- Test orchestration with fixture directories and a decisions repository
  pointed at a local stub. No test reaches the network or needs an API key.
- Test each public method at least once: the success case and each error the
  method owns.
- Assert exact values. Fixtures are controlled, so there is no need for vague
  bounds.
- Use one seed/fixture builder per test module.
- Structure each test as arrange, act, assert without `// Arrange` comments.
- Name tests `it_<verb phrase>` describing what is verified, not the scenario.

## Naming

- Service module: a verb, lowercase (`testing`).
- Public methods: `<verb>_<noun>` (`resolve_rules`, `prepare`, `check`).
- Domain types: nouns (`RuleSet`, `Document`, `Report`).
- Error variants: noun phrases that say what went wrong.

## Related skills

- `@repositories` — the data-access rules.
- `@config` — how config and credentials enter the system.
- `@documentation` — doc comments and plain language.
