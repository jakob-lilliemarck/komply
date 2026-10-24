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
id = "repo.typed-errors"
statement = "A repository defines its own `Error` enum with `thiserror`."
enabled = true

[[rules]]
id = "repo.query-per-file"
statement = "Each query function lives in its own file under `queries/`."
enabled = false
```

| Field | Meaning |
|---|---|
| `path` | Required in the default load mode. A glob, or an array of globs, relative to the working directory. A file is selected when any glob matches. |
| `[files].include` | Optional. Target-relative globs. Defaults to `["**/*"]`. |
| `[files].exclude` | Optional. Target-relative globs. |
| `[[rules]].id` | Required and unique. |
| `[[rules]].statement` | Required and non-empty. |
| `[[rules]].enabled` | Optional. Defaults to `true`. |

A rule is a declarative requirement. Komply wraps it as:

```text
Is the following statement true of the attached code? <statement>
```

A high probability means the statement is true, which is a pass. `noul` is the
probability that the code satisfies the statement.

## Path matching

The tested directory must match one of a rule file's `path` globs. `*` matches
one path segment; `**` crosses segments. Use an array to apply one rule set to
several locations:

```toml
path = [
  "src/services/*/repositories/files",
  "src/services/*/repositories/rules",
]
```

A rule file is selected when any of its globs matches. The tested directory does
not need to match every glob.

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
