# Usage

```text
comply test <dir> [--rules <file-or-dir>] [--dry-run] [--model <id>]
comply --help
comply test --help
```

`<dir>` is resolved from the current working directory. It must be a directory
inside the working directory. Komply reads `.architecture/` from the working
directory only; it does not search parent directories.

## Live run

```sh
export OPENROUTER_API_KEY="..."
comply test src/services/alpha/repositories/tasks
```

A live run prints the header and one pending line per enabled rule, then sends
one request. The final list shows each rule and its probability.

```text
Rules:   .architecture
Target:  src/services/alpha/repositories/tasks
Model:   typesafe/jev-1.13
Files:   3 (60 bytes)

  [pending] repo.no-domain-branching

  [PASS]      repo.no-domain-branching  noul=0.04

0 violations, 0 unsure, 1 pass · $0.00012 · 476 input tokens
```

Colour is used only when standard output is a terminal. Piping the output
turns colour off.

## Dry run

```sh
comply test src/services/alpha/repositories/tasks --dry-run
```

A dry run is offline. It needs no API key. It prints the header, one line per
included file with its byte count, the skipped files, and the request JSON with
the state replaced by a size placeholder.

## Options

| Option | Meaning |
|---|---|
| `--rules <file-or-dir>` | Load exactly this rule file, or every `*.toml` directly inside this directory except `config.toml`. Path matching is skipped. |
| `--dry-run` | Build the request and print it without sending it. |
| `--model <id>` | Override `comply.model` from [config](config.md). |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every rule passed. |
| `1` | At least one violation. |
| `3` | No violations, but at least one unsure. |
| `2` | Any setup, config, network, or document error. |

Errors are printed to standard error as `error: <message>`.

## Related

- [Rules](rules.md)
- [Config](config.md)
- [Ignores](ignores.md)
- [Security](security.md)
