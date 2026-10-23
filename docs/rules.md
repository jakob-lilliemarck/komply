# Rules

Every `*.toml` directly under `.architecture/` is a rule file, except
`config.toml`, which is reserved for [config](config.md). Subdirectories are
not searched. Files are loaded in filename order.

## Rule file

```toml
path = "src/services/*/repositories/*"

[files]
include = ["**/*"]
exclude = ["target/**", "**/*.lock"]

[[rules]]
id = "repo.no-domain-branching"
statement = "There is at least one method implemented on Read, Write or WriteTx that is not a wrapper around search_*, store_* or archive_* functions."
enabled = true

[[rules]]
id = "repo.no-inline-sql"
statement = "The code contains inline SQL."
enabled = false
```

| Field | Meaning |
|---|---|
| `path` | Required in the default load mode. A glob relative to the working directory. |
| `[files].include` | Optional. Target-relative globs. Defaults to `["**/*"]`. |
| `[files].exclude` | Optional. Target-relative globs. |
| `[[rules]].id` | Required and unique. |
| `[[rules]].statement` | Required and non-empty. |
| `[[rules]].enabled` | Optional. Defaults to `true`. |

A rule is a declarative statement. Komply wraps it as:

```text
Is the following statement true of the attached code? <statement>
```

A high probability means the statement is true, which is a violation.

## Path matching

The tested directory must match a rule file's `path` exactly. `*` matches one
path segment; `**` crosses segments.

- `path = "src/services/*"` matches `src/services/alpha`, not
  `src/services/alpha/repositories`.
- `path = "src/services/*/repositories/*"` matches
  `src/services/alpha/repositories/tasks`, not the `repositories` directory
  itself.
- `path = "**"` matches every directory, including the working directory itself.

A deeper directory needs its own rule file or `--rules`.

If several rule files match, their rules combine. File filters also combine: a
file is sent when it matches any selected `include` and no selected `exclude`.
Rule ids must be unique across the combined set.

If no rule file matches, or every selected rule is disabled, the run is an
error. It does not produce a green report.

## Explicit rules

`--rules <file-or-dir>` bypasses path matching. A file is loaded exactly; a
directory loads every `*.toml` directly inside it except `config.toml`. A rule
file loaded this way may omit `path`.

```sh
comply test src/services/alpha/repositories/tasks --rules ./rules.toml
```

## Related

- [Ignores](ignores.md) for the difference between `[files].exclude` and
  `.complyignore`.
- [Config](config.md) for thresholds and limits.
